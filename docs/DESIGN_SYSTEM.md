# Design system

Tokens definidos em `src/index.css` (`@theme`), usados como classes Tailwind (`bg-surface`, `text-muted`, `border-border`…).

| Token | Valor | Uso |
|---|---|---|
| `bg` | #08090B | Fundo principal |
| `bg-2` | #0D1015 | Sidebar, panes, inputs |
| `surface` | #141820 | Cards, dialogs |
| `elevated` | #1B2029 | Hover, menus |
| `border` | #252C37 | Divisores |
| `fg` | #E8EDF5 | Texto primário |
| `muted` / `subtle` | #9AA4B5 / #5F6A7B | Texto secundário / terciário |
| `accent` | #579BFF | Ação primária, foco |
| `accent-2` | #7B8DFF | Acento secundário |
| `success` / `warning` / `error` | #35C98A / #E9AE52 / #F16F78 | Estados |

Tipografia: Geist Variable (UI, 13px base) e Geist Mono (caminhos, código, versões).

Componentes: `Button` (primary · secondary · ghost · outline · danger), `Badge`, `Kbd`, `StatusDot`, `Card`/`CardHeader`,
`Input`, `Textarea`, `Switch`, `Dialog`, `Tooltip`, `DropdownMenu`, `EmptyState`.

Mascote (`<Mascot state=… />`): estados `online`, `thinking`, `working`, `waiting`, `success`, `error`, `sleeping`.
Usado no onboarding, no hero do dashboard e em estados vazios, com moderação. Pode ser desligado em Configurações → Aparência.
