import { create } from "zustand";
import { api, type ProviderCheck, type ProviderStatus } from "@/lib/ipc";

type State = {
  items: ProviderStatus[];
  loaded: boolean;
  loading: boolean;
  verifying: Record<string, boolean>;
  refresh: () => Promise<void>;
  refreshOne: (id: string) => Promise<void>;
  verify: (id: string) => Promise<ProviderCheck>;
};

export const useProviders = create<State>((set, get) => ({
  items: [],
  loaded: false,
  loading: false,
  verifying: {},
  refresh: async () => {
    if (get().loading) return;
    set({ loading: true });
    try {
      set({ items: await api.providersList(), loaded: true });
    } finally {
      set({ loading: false });
    }
  },
  refreshOne: async (id) => {
    const p = await api.providerStatus(id);
    set({ items: get().items.map((x) => (x.id === id ? p : x)) });
  },
  verify: async (id) => {
    set({ verifying: { ...get().verifying, [id]: true } });
    try {
      const check = await api.providerVerify(id);
      await get().refreshOne(id);
      return check;
    } finally {
      set({ verifying: { ...get().verifying, [id]: false } });
    }
  },
}));
