import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useProviderData } from "../../core/ipc";
import { useSettings } from "../../core/settings";
import type { WidgetProps } from "../types";
import { VolumeControl } from "./VolumeControl";
import { clampVolume, toggleMute } from "./volume";
import "./Spotify.css";

export interface SpotifySettings extends Record<string, unknown> {
  showPlaylists: boolean; showArt: boolean;
}

interface Status { client_id: string; redirect_uri: string; logged_in: boolean; login_in_progress: boolean; display_name: string; premium: boolean; error: string | null }
interface Playback {
  is_playing: boolean; progress_ms: number; duration_ms: number; track_id: string | null; title: string; artists: string; album: string;
  art_url: string | null; volume: number; context_uri: string | null; shuffle: boolean; repeat: string; fetched_at: number;
}
interface Playlist { uri: string; name: string; total: number; owner: string; image: string | null }

// 음수는 0:00 으로 — 시계가 잠깐이라도 "-1:59" 를 보여주면 고장으로 읽힌다.
const mmss = (ms: number) => {
  const t = Math.max(0, ms);
  return `${Math.floor(t / 60000)}:${String(Math.floor((t % 60000) / 1000)).padStart(2, "0")}`;
};

/**
 * 진행 시간 — **이 컴포넌트만** 1초마다 다시 그린다.
 * 위쪽(앨범 아트·플레이리스트 목록)까지 매초 리렌더할 이유가 없다.
 *
 * 기준점은 백엔드가 찍은 `fetched_at` 이고, 새 폴링 결과가 올 때마다 0 에서 다시 센다.
 * 예전에는 부모가 들고 있던 `now` 상태에서 `fetched_at` 을 뺐는데, 그 타이머는 **재생 중에만**
 * 돌아 멈춰 있는 동안 `now` 가 과거에 굳었다. 그래서 재생을 누른 직후 첫 1초 동안
 * `now - fetched_at` 이 **음수**가 되어 진행 시간이 "-3:20" 처럼 찍혔다 (사용자 보고).
 */
function Progress({ playback }: { playback: Playback | null }) {
  const [elapsed, setElapsed] = useState(0);
  // 곡이 끝난 직후 Spotify 가 아직 다음 곡으로 넘어가지 않았으면 매초 다시 묻게 된다 — 간격을 둔다.
  const lastPoll = useRef(0);
  const fetchedAt = playback?.fetched_at ?? 0;
  const playing = !!playback?.is_playing;
  const duration = playback?.duration_ms ?? 0;
  const base = playback?.progress_ms ?? 0;

  useEffect(() => {
    setElapsed(0);
    if (!playing) return;
    const t = setInterval(() => {
      const e = Math.max(0, Date.now() - fetchedAt);
      setElapsed(e);
      // 곡이 끝났으면 다음 폴링(최대 5초)을 기다리지 않고 바로 물어본다.
      if (duration > 0 && base + e >= duration && Date.now() - lastPoll.current > 3000) {
        clearInterval(t);
        lastPoll.current = Date.now();
        invoke("spotify_poll").catch(() => {});
      }
    }, 1000);
    return () => clearInterval(t);
  }, [playing, fetchedAt, duration, base]);

  const progress = duration > 0 ? Math.min(duration, base + elapsed) : base + elapsed;
  const pct = duration > 0 ? (progress / duration) * 100 : 0;
  return (
    <div className="sp-progress">
      <div className="sp-bar"><span style={{ width: `${pct}%` }} /></div>
      <div className="sp-times dim"><span>{mmss(progress)}</span><span>{mmss(duration)}</span></div>
    </div>
  );
}

export function Spotify({ instanceId, settings, size }: WidgetProps<SpotifySettings>) {
  const status = useProviderData<Status>("spotify://status", "spotify_status");
  const playback = useProviderData<Playback | null>("spotify://playback", "spotify_last_playback");
  const accent = useSettings((s) => s.instances.find((i) => i.id === instanceId)?.accent);
  const [playlists, setPlaylists] = useState<Playlist[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [volume, setVolume] = useState(50); // 서버 값이 오기 전 기본 표시 (0이면 음소거 아이콘으로 오해)
  const draggingVolume = useRef(false);
  const prevVolume = useRef<number | null>(null);

  // 위젯이 보이는 동안만 백엔드 폴링
  useEffect(() => {
    invoke("spotify_set_active", { active: true }).catch(() => {});
    return () => { invoke("spotify_set_active", { active: false }).catch(() => {}); };
  }, []);
  useEffect(() => {
    if (status?.logged_in) invoke<Playlist[]>("spotify_playlists").then(setPlaylists).catch(() => {});
    else setPlaylists([]);
  }, [status?.logged_in, status?.display_name]);

  const control = async (action: string, arg?: string) => {
    setError(null);
    try { await invoke("spotify_control", { action, arg }); } catch (e) { setError(String(e)); }
  };

  // 서버 볼륨 반영 — 드래그 중에는 내 값이 우선(덮어쓰지 않음)
  useEffect(() => {
    if (playback && !draggingVolume.current) setVolume(playback.volume);
  }, [playback?.volume]);

  const handleVolumeChange = (percent: number, final: boolean) => {
    draggingVolume.current = !final;
    const v = clampVolume(percent);
    setVolume(v);
    control("volume", String(v));
  };
  const handleToggleMute = () => {
    const next = toggleMute({ volume, prevVolume: prevVolume.current });
    prevVolume.current = next.prevVolume;
    setVolume(next.volume);
    control("volume", String(next.volume));
  };

  if (!status) return <div className="dim">불러오는 중…</div>;
  if (!status.logged_in) return <Login status={status} />;

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
          <Progress playback={playback} />
          <div className="sp-controls">
            <button onClick={() => control("previous")} title="이전">⏮</button>
            <button className="sp-play" onClick={() => control(playback?.is_playing ? "pause" : "play")} title={playback?.is_playing ? "일시정지" : "재생"}>
              {playback?.is_playing ? "⏸" : "▶"}
            </button>
            <button onClick={() => control("next")} title="다음">⏭</button>
            <span style={{ flex: 1 }} />
            <VolumeControl instanceId={instanceId} volume={volume} accent={accent} onVolumeChange={handleVolumeChange} onToggleMute={handleToggleMute} />
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
