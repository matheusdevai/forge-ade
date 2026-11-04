import { toast } from "sonner";
import { dirtyFiles } from "./status";

/** Saves every dirty buffer (loads the editor module on demand). */
export async function saveAllWithFeedback(): Promise<boolean> {
  const count = dirtyFiles().length;
  if (count === 0) {
    toast.info("Nada para salvar");
    return true;
  }
  const { saveAll } = await import("./models");
  const { failed } = await saveAll();
  if (failed.length) {
    toast.error(`${failed.length} arquivo(s) não foram salvos`, { description: failed.join(", ") });
    return false;
  }
  toast.success(`${count} arquivo(s) salvos`);
  return true;
}
