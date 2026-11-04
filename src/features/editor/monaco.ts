/**
 * Monaco bootstrap: local workers (no CDN), FORGE theme and language defaults.
 * Imported only from lazily-loaded modules so Monaco stays out of the main bundle.
 */
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import CssWorker from "monaco-editor/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/language/html/html.worker?worker";
import TsWorker from "monaco-editor/language/typescript/ts.worker?worker";

declare global {
  interface Window {
    MonacoEnvironment?: { getWorker(workerId: string, label: string): Worker };
  }
}

window.MonacoEnvironment = {
  getWorker(_id, label) {
    switch (label) {
      case "json":
        return new JsonWorker();
      case "css":
      case "scss":
      case "less":
        return new CssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new HtmlWorker();
      case "typescript":
      case "javascript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

monaco.editor.defineTheme("forge-dark", {
  base: "vs-dark",
  inherit: true,
  rules: [
    { token: "comment", foreground: "5f6a7b", fontStyle: "italic" },
    { token: "keyword", foreground: "7b8dff" },
    { token: "string", foreground: "35c98a" },
    { token: "number", foreground: "e9ae52" },
    { token: "type", foreground: "5cc8e0" },
    { token: "delimiter", foreground: "9aa4b5" },
  ],
  colors: {
    "editor.background": "#0d1015",
    "editor.foreground": "#e8edf5",
    "editorLineNumber.foreground": "#3a4351",
    "editorLineNumber.activeForeground": "#9aa4b5",
    "editor.lineHighlightBackground": "#141820",
    "editor.lineHighlightBorder": "#00000000",
    "editor.selectionBackground": "#579bff40",
    "editor.inactiveSelectionBackground": "#579bff22",
    "editorCursor.foreground": "#579bff",
    "editorIndentGuide.background1": "#1b2029",
    "editorIndentGuide.activeBackground1": "#333b48",
    "editorWhitespace.foreground": "#252c37",
    "editorWidget.background": "#1b2029",
    "editorWidget.border": "#252c37",
    "editorSuggestWidget.background": "#1b2029",
    "editorSuggestWidget.border": "#252c37",
    "editorSuggestWidget.selectedBackground": "#579bff26",
    "editorHoverWidget.background": "#1b2029",
    "editorHoverWidget.border": "#252c37",
    "input.background": "#0d1015",
    "input.border": "#252c37",
    "focusBorder": "#579bff80",
    "scrollbarSlider.background": "#252c3799",
    "scrollbarSlider.hoverBackground": "#333b48",
    "minimap.background": "#0d1015",
    "editorGutter.background": "#0d1015",
  },
});

// Projects are opened file-by-file without a full TS project, so semantic
// diagnostics (unresolved imports, types) would be noise. Keep syntax checks.
for (const defaults of [monaco.typescript.typescriptDefaults, monaco.typescript.javascriptDefaults]) {
  defaults.setDiagnosticsOptions({ noSemanticValidation: true, noSyntaxValidation: false });
  defaults.setCompilerOptions({
    target: monaco.typescript.ScriptTarget.ESNext,
    allowJs: true,
    allowNonTsExtensions: true,
    jsx: monaco.typescript.JsxEmit.Preserve,
    module: monaco.typescript.ModuleKind.ESNext,
    moduleResolution: monaco.typescript.ModuleResolutionKind.NodeJs,
  });
}

export { monaco };
