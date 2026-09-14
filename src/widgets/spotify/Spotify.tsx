import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useProviderData } from "../../core/ipc";
import type { WidgetProps } from "../types";
import "./Spotify.css";

export interface SpotifySettings extends Record<string, unknown> {
  showPlaylists: boolean; showArt: boolean;
}

interface Status { client_id: string; redirect_uri: string; logged_in: boolean; login_in_progress: boolean; display_name: string; premium: boolean; error: string | null }
interface Playback {
  is_playing: boolean; progress_ms: number; duration_ms: number; track_id: string | null; title: string; artists: string; album: string;
  art_url: string | null; device_name: string; volume: number; context_uri: string | null; shuffle: boolean; repeat: string; fetched_at: number;
}
interface Playlist { uri: string; name: string; total: number; owner: string; image: string | null }

const mmss = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor((ms % 60000) / 1000)).padStart(2, "0")}`;

export function Spotify({ settings, size }: WidgetProps<SpotifySettings>) {
  const status = useProviderData<Status>("spotify://status", "spotify_status");
  const playback = useProviderData<Playback | null>("spotify://playback", "spotify_last_playback");
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  // 위젯이 보이는 동안만 백엔드 폴링
  useEffect(() => {
    invoke("spotify_set_active", { active: true }).catch(() => {});
    return () => { invoke("spotify_set_active", { active: false }).catch(() => {}); };
  }, []);
  useEffect(() => {
    if (status?.logged_in) invoke<Playlist[]>("spotify_playlists").then(setPlaylists).catch(() => {});
    else setPlaylists([]);
  }, [status?.logged_in, status?.display_name]);
  // 진행바 보간 (1초)
  useEffect(() => {
    if (!playback?.is_playing) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [playback?.is_playing]);

  const control = async (action: string, arg?: string) => {
    setError(null);
    try { await invoke("spotify_control", { action, arg }); } catch (e) { setError(String(e)); }
  };

  if (!status) return <div className="dim">불러오는 중…</div>;
  if (!status.logged_in) return <Login status={status} />;

  const progress = playback ? Math.min(playback.duration_ms, playback.progress_ms + (playback.is_playing ? now - playback.fetched_at : 0)) : 0;
  const pct = playback?.duration_ms ? (progress / playback.duration_ms) * 100 : 0;
  const compact = size.h < 170;
  const artSize = Math.max(48, Math.min(96, size.h * 0.45));

  return (
    <div className="sp">
      <div className="sp-now">
        {settings.showArt && (
          <div className="sp-art" style={{ width: artSize, height: artSize }}>
            {playback?.art_url ? <img src={playback.art_url} alt="" /> : <span>♪</span>}
          </div>
        )}
        <div className="sp-info">
          {playback?.title ? (
            <>
              <div className="sp-title" title={playback.title}>{playback.title}</div>
              <div className="sp-artist dim" title={playback.album}>{playback.artists}</div>
            </>
          ) : (
            <div className="dim">재생 중인 곡 없음{status.display_name ? ` · ${status.display_name}` : ""}</div>
          )}
          <div className="sp-progress" onClick={undefined}>
            <div className="sp-bar"><span style={{ width: `${pct}%` }} /></div>
            <div className="sp-times dim"><span>{mmss(progress)}</span><span>{playback ? mmss(playback.duration_ms) : "0:00"}</span></div>
          </div>
          <div className="sp-controls">
            <button onClick={() => control("previous")} title="이전">⏮</button>
            <button className="sp-play" onClick={() => control(playback?.is_playing ? "pause" : "play")} title={playback?.is_playing ? "일시정지" : "재생"}>
              {playback?.is_playing ? "⏸" : "▶"}
            </button>
            <button onClick={() => control("next")} title="다음">⏭</button>
            {!compact && playback?.device_name && <span className="sp-device dim" title="재생 디바이스">{playback.device_name}</span>}
            <span style={{ flex: 1 }} />
            <button className="sp-icon" title="새로고침" onClick={() => invoke("spotify_refresh").catch((e) => setError(String(e)))}>↻</button>
            <button className="sp-icon" title="로그아웃" onClick={() => invoke("spotify_logout")}>⏻</button>
          </div>
        </div>
      </div>
      {error && <div className="sp-error">{error}</div>}
      {!status.premium && !error && <div className="sp-error dim">Premium 계정이 아니면 재생 제어가 동작하지 않습니다</div>}

      {!compact && settings.showPlaylists && (
        <div className="sp-lists">
          {playlists.map((p) => (
            <button key={p.uri} className={`sp-list ${playback?.context_uri === p.uri ? "on" : ""}`} onClick={() => control("play_context", p.uri)} title={`${p.owner} · ${p.total}곡`}>
              {p.image ? <img src={p.image} alt="" /> : <span className="sp-list-ph">♫</span>}
              <span className="sp-list-name">{p.name}</span>
              {p.total > 0 && <span className="dim">{p.total}</span>}
            </button>
          ))}
          {playlists.length === 0 && <div className="dim">플레이리스트 없음</div>}
        </div>
      )}
    </div>
  );
}

function Login({ status }: { status: Status }) {
  const [clientId, setClientId] = useState(status.client_id);
  const [redirect, setRedirect] = useState(status.redirect_uri || "http://127.0.0.1:8888/callback");
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { if (status.client_id) setClientId(status.client_id); }, [status.client_id]);

  const login = async () => {
    setErr(null);
    try { await invoke("spotify_login", { clientId, redirectUri: redirect }); } catch (e) { setErr(String(e)); }
  };
  return (
    <div className="sp-login">
      <div className="sp-login-title">Spotify 로그인</div>
      <div className="dim">
        <a onClick={() => openUrl("https://developer.spotify.com/dashboard")}>developer.spotify.com/dashboard</a> 에서 앱을 만들고
        Redirect URI 에 <code>{redirect}</code> 를 추가한 뒤 Client ID 를 붙여넣으세요. (PKCE — Secret 불필요)
      </div>
      <input placeholder="Client ID" value={clientId} onChange={(e) => setClientId(e.target.value)} onKeyDown={(e) => e.key === "Enter" && login()} />
      <input placeholder="Redirect URI" value={redirect} onChange={(e) => setRedirect(e.target.value)} />
      {status.login_in_progress ? (
        <div className="sp-login-row">
          <span className="dim">브라우저에서 승인을 기다리는 중…</span>
          <button onClick={() => invoke("spotify_cancel_login")}>취소</button>
        </div>
      ) : (
        <button className="primary" onClick={login} disabled={!clientId.trim()}>브라우저로 로그인</button>
      )}
      {(err || status.error) && <div className="sp-error">{err ?? status.error}</div>}
    </div>
  );
}
