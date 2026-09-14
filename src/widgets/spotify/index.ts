import type { WidgetDefinition } from "../types";
import { Spotify, type SpotifySettings } from "./Spotify";

export const spotifyWidget: WidgetDefinition<SpotifySettings> = {
  id: "spotify",
  title: "Spotify",
  icon: "🎵",
  component: Spotify,
  defaultSize: { w: 360, h: 320 },
  minSize: { w: 260, h: 120 },
  settingsSchema: [
    { key: "showArt", label: "앨범 아트", type: "boolean", default: true },
    { key: "showPlaylists", label: "플레이리스트 목록", type: "boolean", default: true },
  ],
};
