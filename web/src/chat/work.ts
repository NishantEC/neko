// The one pure piece of the work log that is about words: a step's line.

/** "Ran `git status`" -> verb "Ran", target "git status" (backticks removed, mono). */
export function splitStep(text: string): { verb: string; target: string; code: boolean } {
  const space = text.indexOf(" ");
  if (space < 0) return { verb: text, target: "", code: false };
  const rest = text.slice(space + 1).trim();
  const code = rest.startsWith("`") && rest.includes("`", 1);
  return { verb: text.slice(0, space), target: code ? rest.replace(/^`|`$/g, "") : rest, code };
}
