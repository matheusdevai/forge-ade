import { create } from "zustand";
import { api, type SettingKey, type Settings } from "@/lib/ipc";

const DEFAULTS: Settings = {
  "onboarding.completed": false,
  "session.restore": true,
  "session.lastWorkspaceId": null,
  "ui.mascot": true,
  "ui.reduceMotion": false,
  "workspace.defaultDir": null,
  "voice.locale": null,
  "voice.onDevice": false,
  "voice.speak": false,
  "voice.autoSend": false,
};

type State = {
  values: Settings;
  loaded: boolean;
  load: () => Promise<void>;
  set: <K extends SettingKey>(key: K, value: Settings[K]) => Promise<void>;
};

export const useSettings = create<State>((set, get) => ({
  values: DEFAULTS,
  loaded: false,
  load: async () => {
    const values = await api.settingsGetAll();
    set({ values: { ...DEFAULTS, ...values }, loaded: true });
  },
  set: async (key, value) => {
    const prev = get().values;
    set({ values: { ...prev, [key]: value } });
    try {
      await api.settingsSet(key, value);
    } catch (e) {
      set({ values: prev });
      throw e;
    }
  },
}));
