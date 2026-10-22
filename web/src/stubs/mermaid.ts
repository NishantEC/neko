// Streamdown can draw ```mermaid blocks, and pulls in the whole mermaid library
// (about 30 MB of chunks in dist) to do it. Neko's replies do not carry
// diagrams, so the library is swapped for this stub in vite.config.ts; a
// mermaid block renders as the code it is.
const mermaid = {
  initialize() {},
  async render(): Promise<{ svg: string }> {
    throw new Error("Diagrams are not bundled in Neko's web UI.");
  },
};
export default mermaid;
