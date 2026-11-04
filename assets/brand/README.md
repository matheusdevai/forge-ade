# Brand assets (oficiais)

Coloque aqui **somente** os arquivos oficiais fornecidos pelo proprietário. Não substitua por imagens genéricas.

| Arquivo | Obrigatório | Uso |
|---|---|---|
| `forge-logo.png` | sim | Logo oficial (símbolo F metálico + wordmark FORGE). Sidebar, dashboard, onboarding. |
| `forge-mascot.png` | sim | Mascote oficial (robô, olhos azuis, roupa preta, símbolo F). Fundo transparente de preferência. |
| `forge-symbol.png` | não | Recorte só do símbolo F para espaços compactos. |
| `mascot/<estado>.png` | não | Arte oficial por estado: `online`, `thinking`, `working`, `waiting`, `success`, `error`, `sleeping`. |

Sem arte por estado, o FORGE aplica apenas um tratamento sutil (brilho/tonalidade) sobre o mascote base. O personagem nunca é redesenhado.

Depois de adicionar a logo, gere os ícones do app:

```bash
npx tauri icon assets/brand/forge-logo.png -o src-tauri/icons   # idealmente uma versão quadrada 1024×1024 do símbolo
```

Os ícones atuais em `src-tauri/icons/` são um **tile neutro provisório** (sem símbolo), usado só para o bundle compilar.
