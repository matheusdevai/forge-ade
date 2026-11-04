/**
 * Built-in layout templates. They only use pane kinds that are implemented,
 * and are instantiated with fresh ids every time they are applied.
 */
import { createPane, docFromRoot, split, type LayoutDoc } from "./layout";

export type BuiltinPreset = { id: string; name: string; description: string; build: () => LayoutDoc };

export const BUILTIN_PRESETS: BuiltinPreset[] = [
  {
    id: "builtin:focus",
    name: "Foco",
    description: "Um único pane",
    build: () => docFromRoot(createPane("explorer")),
  },
  {
    id: "builtin:side-by-side",
    name: "Lado a lado",
    description: "Dois panes em colunas",
    build: () => docFromRoot(split("horizontal", [createPane("explorer"), createPane("overview")])),
  },
  {
    id: "builtin:dev",
    name: "Desenvolvimento",
    description: "Arquivos + overview + terminal",
    build: () =>
      docFromRoot(split("horizontal", [createPane("explorer"), split("vertical", [createPane("overview"), createPane("terminal")], [45, 55])], [58, 42])),
  },
  {
    id: "builtin:agent",
    name: "Agente",
    description: "Agente + editor + Git",
    build: () => docFromRoot(split("horizontal", [createPane("agent"), split("vertical", [createPane("editor"), createPane("git")], [55, 45])], [45, 55])),
  },
  {
    id: "builtin:review",
    name: "Revisão",
    description: "Git + editor + terminal",
    build: () => docFromRoot(split("horizontal", [createPane("git"), split("vertical", [createPane("editor"), createPane("terminal")], [65, 35])], [55, 45])),
  },
  {
    id: "builtin:three-columns",
    name: "Três colunas",
    description: "Três panes em colunas",
    build: () => docFromRoot(split("horizontal", [createPane("explorer"), createPane("explorer"), createPane("overview")])),
  },
  {
    id: "builtin:grid",
    name: "Grade 2×2",
    description: "Quatro panes em grade",
    build: () =>
      docFromRoot(
        split("vertical", [
          split("horizontal", [createPane("explorer"), createPane("explorer")]),
          split("horizontal", [createPane("explorer"), createPane("overview")]),
        ]),
      ),
  },
  {
    id: "builtin:main-side",
    name: "Principal + lateral",
    description: "Pane principal com coluna lateral dividida",
    build: () =>
      docFromRoot(split("horizontal", [createPane("explorer"), split("vertical", [createPane("overview"), createPane("explorer")])], [68, 32])),
  },
];
