import { assetUrl } from './api';
import type { AchievementView } from './types';

/** Icon URL for an achievement: the colour icon once unlocked, the grey one
 *  while locked. Local schema icons are app-data paths served via `asset:`. */
export function achievementIconSrc(a: AchievementView): string | null {
  const src = a.unlocked_at !== null ? a.icon : (a.icon_gray ?? a.icon);
  if (!src) return null;
  return src.startsWith('http') ? src : assetUrl(src);
}
