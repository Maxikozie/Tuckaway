// Shapes of the JSON the Rust backend sends (serde forms from `hidpp`).

export type Binding =
  | { type: "mouse"; button: number; mask?: number }
  | { type: "key"; keys: string }
  | { type: "consumer"; key: string }
  | { type: "disabled" }
  | { type: "function"; function: string; data?: number; reserved?: number }
  | { type: "macro"; raw: string }
  | { type: "unassigned" }
  | { type: "unknown"; raw: string };

export type Led =
  | { mode: "off" }
  | { mode: "static"; color: string; ramp?: number }
  | { mode: "cycle"; period_ms: number; brightness: number }
  | { mode: "breathing"; color: string; period_ms: number; waveform?: number; brightness: number }
  | { mode: "raw"; raw: string };

export interface Profile {
  report_interval_ms: number;
  default_dpi_index: number;
  shift_dpi_index: number;
  dpi: number[];
  color: string;
  power_mode: number;
  angle_snapping: number;
  buttons: Binding[];
  gshift_buttons: Binding[];
  name: string;
  leds: Led[];
}

export type Group = "mouse" | "shortcuts" | "media" | "dpi" | "profiles" | "disabled" | "other";

export interface Action {
  label: string;
  group: Group;
  keys?: string;
  editable: boolean;
  binding: Binding;
}

export interface ProfileView {
  index: number;
  sector: number;
  enabled: boolean;
  from_rom: boolean;
  profile: Profile;
  actions: { buttons: Action[]; gshift: Action[] };
}

export interface LedZone {
  index: number;
  location: number;
  effects: number[];
}

export interface Snapshot {
  name: string;
  id: string;
  firmware: string[];
  mock: boolean;
  product_id: number;
  profiles: ProfileView[];
  active_sector: number;
  mode: number;
  dpi_values: number[];
  rates_ms: number[];
  led_zones: LedZone[];
  slots: { slot: number; label: string; location: string }[];
  has_gshift: boolean;
  has_dpi_shift: boolean;
}

export type DeviceEv =
  | { kind: "searching" }
  | { kind: "connected"; data: Snapshot }
  | { kind: "disconnected"; data: string }
  | { kind: "active"; data: { sector: number; mode: number } }
  | { kind: "busy"; data: string };

export type Tab = "buttons" | "performance" | "lighting";
