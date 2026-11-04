import { create } from "zustand";
import { api, type AgentProfile, type ProfileInput } from "@/lib/ipc";

type State = {
  profiles: AgentProfile[];
  loaded: boolean;
  refresh: () => Promise<void>;
  create: (input: ProfileInput) => Promise<AgentProfile>;
  update: (id: string, input: ProfileInput) => Promise<AgentProfile>;
  remove: (id: string) => Promise<void>;
  restore: (id: string) => Promise<AgentProfile>;
};

export const useAgents = create<State>((set, get) => ({
  profiles: [],
  loaded: false,
  refresh: async () => set({ profiles: await api.agentProfilesList(), loaded: true }),
  create: async (input) => {
    const p = await api.agentProfileCreate(input);
    await get().refresh();
    return p;
  },
  update: async (id, input) => {
    const p = await api.agentProfileUpdate(id, input);
    await get().refresh();
    return p;
  },
  remove: async (id) => {
    await api.agentProfileDelete(id);
    await get().refresh();
  },
  restore: async (id) => {
    const p = await api.agentProfileRestore(id);
    await get().refresh();
    return p;
  },
}));

export const PERMISSION_INFO = {
  read: { label: "Somente leitura", short: "leitura", description: "Lê e analisa; nunca altera arquivos." },
  edit: { label: "Pode editar", short: "edição", description: "Pode editar arquivos no diretório da execução." },
  full: { label: "Acesso total", short: "total", description: "Ferramentas sem restrição (inclui comandos). Exige confirmação a cada execução." },
} as const;
