//! Spotify Web API 호출 (필요한 것만). 오류는 사람이 읽을 메시지로 바꾼다.

use serde::Serialize;
use serde_json::{json, Value};

const BASE: &str = "https://api.spotify.com/v1";

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("인증이 만료되었습니다")]
    Unauthorized,
    #[error("제어 권한이 없습니다 (Premium 계정이 아니거나 제한된 동작)")]
    Forbidden,
    #[error("활성 디바이스가 없습니다. Spotify 앱에서 먼저 재생을 시작하세요")]
    NoActiveDevice,
    #[error("요청이 너무 많습니다. {0}초 후 재시도")]
    RateLimited(u64),
    #[error("Spotify API 오류 ({0}): {1}")]
    Status(u16, String),
    #[error("네트워크 오류: {0}")]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Playback {
    pub is_playing: bool,
    pub progress_ms: u64,
    pub duration_ms: u64,
    pub track_id: Option<String>,
    pub title: String,
    pub artists: String,
    pub album: String,
    pub art_url: Option<String>,
    pub device_name: String,
    pub volume: u32,
    pub context_uri: Option<String>,
    pub shuffle: bool,
    pub repeat: String,
    /// unix ms — 프론트가 진행바를 보간할 기준
    pub fetched_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Playlist {
    pub uri: String,
    pub name: String,
    pub total: u64,
    pub owner: String,
    pub image: Option<String>,
}

pub struct Api<'a> {
    pub http: &'a reqwest::Client,
    pub access_token: &'a str,
}

impl<'a> Api<'a> {
    async fn check(res: reqwest::Response) -> Result<reqwest::Response, ApiError> {
        let status = res.status();
        match status.as_u16() {
            200..=299 => Ok(res),
            401 => Err(ApiError::Unauthorized),
            403 => Err(ApiError::Forbidden),
            404 => Err(ApiError::NoActiveDevice),
            429 => {
                let retry = res.headers().get("Retry-After").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(5);
                Err(ApiError::RateLimited(retry))
            }
            code => Err(ApiError::Status(code, res.text().await.unwrap_or_default())),
        }
    }

    async fn get(&self, path: &str) -> Result<Option<Value>, ApiError> {
        let res = self.http.get(format!("{BASE}{path}")).bearer_auth(self.access_token).send().await?;
        let res = Self::check(res).await?;
        if res.status().as_u16() == 204 {
            return Ok(None);
        }
        let text = res.text().await?;
        if text.trim().is_empty() {
            return Ok(None);
        }
        Ok(serde_json::from_str(&text).ok())
    }

    async fn send(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<(), ApiError> {
        let mut req = self.http.request(method, format!("{BASE}{path}")).bearer_auth(self.access_token);
        if let Some(b) = body {
            req = req.json(&b);
        } else {
            req = req.header("Content-Length", "0");
        }
        Self::check(req.send().await?).await.map(|_| ())
    }

    pub async fn me(&self) -> Result<(String, bool), ApiError> {
        let v = self.get("/me").await?.unwrap_or(Value::Null);
        let name = v.get("display_name").and_then(Value::as_str).or_else(|| v.get("id").and_then(Value::as_str)).unwrap_or("").to_string();
        let premium = v.get("product").and_then(Value::as_str) == Some("premium");
        Ok((name, premium))
    }

    pub async fn playback(&self) -> Result<Option<Playback>, ApiError> {
        let Some(v) = self.get("/me/player?additional_types=track,episode").await? else { return Ok(None) };
        Ok(Some(parse_playback(&v)))
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>, ApiError> {
        let mut out = Vec::new();
        let mut offset = 0;
        loop {
            let Some(page) = self.get(&format!("/me/playlists?limit=50&offset={offset}")).await? else { break };
            let items = page.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
            for it in &items {
                if it.is_null() { continue; }
                let s = |k: &str| it.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                out.push(Playlist {
                    uri: s("uri"),
                    name: if s("name").is_empty() { "(제목 없음)".into() } else { s("name") },
                    total: it.pointer("/tracks/total").and_then(Value::as_u64).unwrap_or(0),
                    owner: it.pointer("/owner/display_name").and_then(Value::as_str).unwrap_or("").to_string(),
                    image: it.get("images").and_then(Value::as_array).and_then(|a| a.last()).and_then(|i| i.get("url")).and_then(Value::as_str).map(str::to_string),
                });
            }
            if page.get("next").map_or(true, Value::is_null) || items.is_empty() {
                break;
            }
            offset += 50;
        }
        Ok(out)
    }

    pub async fn play(&self) -> Result<(), ApiError> { self.send(reqwest::Method::PUT, "/me/player/play", None).await }
    pub async fn pause(&self) -> Result<(), ApiError> { self.send(reqwest::Method::PUT, "/me/player/pause", None).await }
    pub async fn next(&self) -> Result<(), ApiError> { self.send(reqwest::Method::POST, "/me/player/next", None).await }
    pub async fn previous(&self) -> Result<(), ApiError> { self.send(reqwest::Method::POST, "/me/player/previous", None).await }
    pub async fn play_context(&self, uri: &str) -> Result<(), ApiError> {
        self.send(reqwest::Method::PUT, "/me/player/play", Some(json!({ "context_uri": uri }))).await
    }
    pub async fn set_volume(&self, percent: u32) -> Result<(), ApiError> {
        self.send(reqwest::Method::PUT, &format!("/me/player/volume?volume_percent={}", percent.min(100)), None).await
    }
    pub async fn set_shuffle(&self, on: bool) -> Result<(), ApiError> {
        self.send(reqwest::Method::PUT, &format!("/me/player/shuffle?state={on}"), None).await
    }

    /// 활성 디바이스가 없을 때: 첫 디바이스로 재생을 옮긴다. 성공하면 true.
    pub async fn wake_device(&self) -> Result<bool, ApiError> {
        let v = self.get("/me/player/devices").await?.unwrap_or(Value::Null);
        let devices = v.get("devices").and_then(Value::as_array).cloned().unwrap_or_default();
        let pick = devices.iter().find(|d| d.get("is_active").and_then(Value::as_bool).unwrap_or(false)).or(devices.first());
        let Some(id) = pick.and_then(|d| d.get("id")).and_then(Value::as_str) else { return Ok(false) };
        self.send(reqwest::Method::PUT, "/me/player", Some(json!({ "device_ids": [id], "play": false }))).await?;
        Ok(true)
    }
}

pub fn parse_playback(v: &Value) -> Playback {
    let item = v.get("item").cloned().unwrap_or(Value::Null);
    let s = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let artists = item
        .get("artists")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|x| x.get("name").and_then(Value::as_str)).collect::<Vec<_>>().join(", "))
        .or_else(|| item.pointer("/show/publisher").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let album_obj = item.get("album").or_else(|| item.get("show")).cloned().unwrap_or(Value::Null);
    let art_url = album_obj
        .get("images")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(|i| i.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let device = v.get("device").cloned().unwrap_or(Value::Null);
    Playback {
        is_playing: v.get("is_playing").and_then(Value::as_bool).unwrap_or(false),
        progress_ms: v.get("progress_ms").and_then(Value::as_u64).unwrap_or(0),
        duration_ms: item.get("duration_ms").and_then(Value::as_u64).unwrap_or(0),
        track_id: item.get("id").and_then(Value::as_str).map(str::to_string),
        title: s(&item, "name"),
        artists,
        album: s(&album_obj, "name"),
        art_url,
        device_name: s(&device, "name"),
        volume: device.get("volume_percent").and_then(Value::as_u64).unwrap_or(0) as u32,
        context_uri: v.pointer("/context/uri").and_then(Value::as_str).map(str::to_string),
        shuffle: v.get("shuffle_state").and_then(Value::as_bool).unwrap_or(false),
        repeat: s(v, "repeat_state"),
        fetched_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_playback_json() {
        let v = json!({
            "is_playing": true, "progress_ms": 1234, "shuffle_state": true, "repeat_state": "off",
            "device": {"name": "PC", "volume_percent": 55},
            "context": {"uri": "spotify:playlist:abc"},
            "item": {"id": "t1", "name": "Song", "duration_ms": 200000,
                     "artists": [{"name": "A"}, {"name": "B"}],
                     "album": {"name": "Alb", "images": [{"url": "big"}, {"url": "small"}]}}
        });
        let p = parse_playback(&v);
        assert!(p.is_playing);
        assert_eq!(p.title, "Song");
        assert_eq!(p.artists, "A, B");
        assert_eq!(p.album, "Alb");
        assert_eq!(p.art_url.as_deref(), Some("big"));
        assert_eq!(p.volume, 55);
        assert_eq!(p.context_uri.as_deref(), Some("spotify:playlist:abc"));
        assert_eq!(p.duration_ms, 200000);
    }

    #[test]
    fn parses_empty_item() {
        let p = parse_playback(&json!({"is_playing": false}));
        assert_eq!(p.title, "");
        assert!(p.art_url.is_none());
    }
}
