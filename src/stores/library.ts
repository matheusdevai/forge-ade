import { create } from "zustand";
import { api, type Recipe, type Squad } from "@/lib/ipc";

type State = {
  squads: Squad[];
  recipes: Recipe[];
  loaded: boolean;
  refresh: () => Promise<void>;
};

export const useLibrary = create<State>((set) => ({
  squads: [],
  recipes: [],
  loaded: false,
  refresh: async () => {
    const [squads, recipes] = await Promise.all([api.squadsList(), api.recipesList()]);
    set({ squads, recipes, loaded: true });
  },
}));

/** Client-side preview of `{{key}}` placeholders (the backend renders the real mission). */
export function renderTemplate(template: string, values: Record<string, string>): string {
  return template.replace(/\{\{\s*([A-Za-z0-9_]+)\s*\}\}/g, (_, k: string) => values[k]?.trim() || "não especificado");
}
