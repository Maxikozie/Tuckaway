// Thin wrappers over the Tauri commands in crates/app/src/lib.rs.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Action, DeviceEv, Led, Profile, ProfileView } from "./types";

export const api = {
  deviceState: () => invoke<DeviceEv>("device_state"),
  onDevice: (f: (e: DeviceEv) => void): Promise<UnlistenFn> =>
    listen<DeviceEv>("device", (e) => f(e.payload)),
  catalog: () => invoke<Action[]>("catalog"),
  shortcut: (codes: string[]) => invoke<Action | null>("shortcut", { codes }),
  describeProfile: (profile: Profile) =>
    invoke<ProfileView["actions"]>("describe_profile", { profile }),
  lightingServiceRunning: () => invoke<boolean>("lighting_service_running"),
  reload: () => invoke("reload"),
  apply: (edits: { index: number; enabled: boolean; profile: Profile }[], active: number) =>
    invoke<{ sectors: number }>("apply", { edits, active }),
  activate: (sector: number) => invoke("activate", { sector }),
  ledPreview: (zone: number, led: Led) => invoke("led_preview", { zone, led }),
  ledRelease: () => invoke("led_release"),
  backupNow: () => invoke<{ path: string | null }>("backup_now"),
  restore: (path: string) => invoke<{ sectors: number }>("restore", { path }),
  factoryReset: () => invoke<{ sectors: number }>("factory_reset"),
  exportProfile: (profile: Profile, path: string) => invoke("export_profile", { profile, path }),
  importProfile: (index: number, path: string) =>
    invoke<{ profile: Profile; actions: ProfileView["actions"] }>("import_profile", { index, path }),
  backupDir: () => invoke<string>("backup_dir"),
};
