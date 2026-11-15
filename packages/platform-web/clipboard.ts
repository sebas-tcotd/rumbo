/** Write text on a player gesture, returning false if the browser denies clipboard access. */
export async function copyText(target: Window, text: string): Promise<boolean> {
  try {
    const clipboard = target.navigator.clipboard;
    if (!clipboard) return false;
    await clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}
