// A single newline inside a paragraph becomes a line break, as in T3 Code's
// user messages (it uses the remark-breaks package; this is the same rule in
// a few lines, so no dependency is added).
interface Node {
  type: string;
  value?: string;
  children?: Node[];
}

function split(node: Node) {
  if (!node.children) return;
  const out: Node[] = [];
  for (const child of node.children) {
    if (child.type === "text" && child.value?.includes("\n")) {
      child.value.split("\n").forEach((part, i) => {
        if (i > 0) out.push({ type: "break" });
        if (part) out.push({ type: "text", value: part });
      });
    } else {
      split(child);
      out.push(child);
    }
  }
  node.children = out;
}

export default function remarkBreaks() {
  return (tree: Node) => split(tree);
}
