// App state. The backend snapshot is the base; edits go to a working copy,
// and Apply sends the working profiles that differ.

import { api } from "./api";
import { accentFrom, ledColor } from "./color";
import { changes } from "./names";
import type { Action, DeviceEv, Led, ProfileView, Snapshot, Tab } from "./types";

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);

export type Toast = { id: number; text: string; error?: boolean };

class AppState {
  status = $state<"searching" | "connected" | "disconnected">("searching");
  note = $state("");
  snap = $state<Snapshot | null>(null);
  base = $state<ProfileView[]>([]);
  work = $state<ProfileView[]>([]);
  selected = $state(0);
  tab = $state<Tab>("buttons");
  gshift = $state(false);
  busy = $state<string | null>(null);
  toasts = $state<Toast[]>([]);
  catalog = $state<Action[]>([]);
  zone = $state(1);
  lightingBlocked = $state(false);

  activeSector = $derived(this.snap?.active_sector ?? 0);
  current = $derived(this.work.find((p) => p.index === this.selected) ?? null);
  pending = $derived(changes(this.base, this.work));
  dirty = $derived(this.pending.length > 0);
  // follows the profile on screen, including unsaved lighting edits
  accent = $derived(accentFrom(ledColor(this.current?.profile.leds[0])));

  async start() {
    await api.onDevice((e) => this.onDevice(e));
    this.onDevice(await api.deviceState());
    this.catalog = await api.catalog();
  }

  onDevice(e: DeviceEv) {
    switch (e.kind) {
      case "searching":
        if (this.status !== "connected") this.status = "searching";
        break;
      case "disconnected":
        this.status = "disconnected";
        this.note = e.data;
        break;
      case "busy":
        this.busy = e.data;
        break;
      case "active":
        if (this.snap) {
          this.snap.active_sector = e.data.sector;
          this.snap.mode = e.data.mode;
        }
        break;
      case "connected":
        this.load(e.data);
        break;
    }
  }

  load(s: Snapshot) {
    const keepEdits = this.dirty && same(this.base, s.profiles);
    this.snap = s;
    this.base = clone(s.profiles);
    if (!keepEdits) this.work = clone(s.profiles);
    if (!this.work.some((p) => p.index === this.selected)) this.selected = 0;
    if (this.status !== "connected") {
      const active = s.profiles.find((p) => p.sector === s.active_sector);
      if (active) this.selected = active.index;
    }
    this.status = "connected";
  }

  isChanged(index: number) {
    const w = this.work.find((p) => p.index === index);
    const b = this.base.find((p) => p.index === index);
    return !!w && !!b && !same(w, b);
  }

  setBinding(slot: number, action: Action, layer: "buttons" | "gshift" = this.gshift ? "gshift" : "buttons") {
    const p = this.current;
    if (!p) return;
    if (layer === "gshift") {
      p.profile.gshift_buttons[slot] = clone(action.binding);
      p.actions.gshift[slot] = clone(action);
    } else {
      p.profile.buttons[slot] = clone(action.binding);
      p.actions.buttons[slot] = clone(action);
    }
  }

  setEnabled(index: number, enabled: boolean) {
    const p = this.work.find((x) => x.index === index);
    if (p) p.enabled = enabled;
  }

  async discard() {
    this.work = clone(this.base);
    await api.ledRelease().catch(() => {});
  }

  async apply() {
    const edits = this.work
      .filter((p) => this.isChanged(p.index))
      .map((p) => ({ index: p.index, enabled: p.enabled, profile: p.profile }));
    const n = this.pending.length;
    this.busy = "Writing";
    try {
      await api.apply(edits, this.activeSector);
      this.toast(n === 1 ? "Applied 1 change" : `Applied ${n} changes`);
      return true;
    } catch (e) {
      this.toast(String(e), true);
      return false;
    } finally {
      this.busy = null;
    }
  }

  async activate(index: number) {
    const p = this.work.find((x) => x.index === index);
    if (!p) return;
    try {
      await api.activate(p.sector);
    } catch (e) {
      this.toast(String(e), true);
    }
  }

  async run(label: string, f: () => Promise<string | null>) {
    this.busy = label;
    try {
      const msg = await f();
      if (msg) this.toast(msg);
    } catch (e) {
      this.toast(String(e), true);
    } finally {
      this.busy = null;
    }
  }

  /** Edit an LED zone of the profile being edited and mirror it on the mouse. */
  setLed(zone: number, led: Led) {
    const p = this.current;
    if (!p) return;
    p.profile.leds[zone] = led;
    const pristine = this.work.every((w) => same(w.profile.leds, this.base.find((b) => b.index === w.index)?.profile.leds));
    this.queue = pristine ? { kind: "release" } : { kind: "preview", zone, led: clone(led) };
    this.pump();
  }

  // Only the newest preview is sent; a drag produces many edits per frame.
  private queue: { kind: "preview"; zone: number; led: Led } | { kind: "release" } | null = null;
  private sending = false;
  private async pump() {
    if (this.sending || this.status !== "connected") return;
    this.sending = true;
    while (this.queue) {
      const q = this.queue;
      this.queue = null;
      await (q.kind === "preview" ? api.ledPreview(q.zone, q.led) : api.ledRelease()).catch(() => {});
    }
    this.sending = false;
  }

  async checkLighting() {
    this.lightingBlocked = await api.lightingServiceRunning().catch(() => false);
  }

  private nextToast = 1;
  toast(text: string, error = false) {
    const t = { id: this.nextToast++, text, error };
    this.toasts = [...this.toasts, t];
    setTimeout(() => (this.toasts = this.toasts.filter((x) => x.id !== t.id)), error ? 6000 : 3200);
  }
}

export const app = new AppState();
