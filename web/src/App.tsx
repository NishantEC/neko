import { useEffect, useState } from "react";
import { Home } from "./Home";
import { Board, Home as HomeIcon, Puzzle } from "./icons";
import { PluginBoundary, usePlugins } from "./plugins";
import { useWorkbench } from "./useStore";

export function App() {
  const { connected, unauthorized } = useWorkbench();
  const { pages, problems } = usePlugins();
  const [route, setRoute] = useState(() => location.hash.slice(1) || "home");
  useEffect(() => {
    const onHash = () => setRoute(location.hash.slice(1) || "home");
    addEventListener("hashchange", onHash);
    return () => removeEventListener("hashchange", onHash);
  }, []);

  if (unauthorized) {
    return (
      <div className="gate">
        <h1>Neko</h1>
        <p>This page is not signed in to the daemon. Open Neko’s “Home (web)” page, or the address with <code>?token=</code> from the <code>web-token</code> file.</p>
      </div>
    );
  }

  const page = pages.find((p) => p.key === route);
  const go = (to: string) => () => {
    location.hash = to === "home" ? "" : to;
    setRoute(to);
  };

  return (
    <div className="shell">
      <nav className="side" aria-label="Pages">
        <div className="brand">Neko</div>
        <button className="nav" data-active={route === "home"} onClick={go("home")}>
          <HomeIcon /> Home
        </button>
        {(pages.length > 0 || problems.length > 0) && <div className="nav-label">Plugins</div>}
        {pages.map((p) => (
          <button key={p.key} className="nav" data-active={route === p.key} onClick={go(p.key)}>
            <Board /> {p.title}
          </button>
        ))}
        {problems.map((p) => (
          <button key={p.plugin} className="nav problem" title={p.message} onClick={go(`problem:${p.plugin}`)}>
            <Puzzle /> {p.plugin}
          </button>
        ))}
        <div className="grow" />
        <div className="status"><span className="dot" data-on={connected} />{connected ? "Connected" : "Reconnecting…"}</div>
      </nav>
      <main className="main">
        {route === "home" && <Home />}
        {page && (
          <PluginBoundary plugin={page.plugin} key={page.key}>
            <page.component />
          </PluginBoundary>
        )}
        {route.startsWith("problem:") && (
          <div className="plugin-error">
            <h2>The “{route.slice(8)}” plugin did not load</h2>
            <pre>{problems.find((p) => p.plugin === route.slice(8))?.message ?? "It loads now."}</pre>
          </div>
        )}
        {route !== "home" && !page && !route.startsWith("problem:") && <p className="muted center pad">This page is not available (yet).</p>}
      </main>
    </div>
  );
}
