import { AppShell } from "@/shell/AppShell";
import { useWorkbench } from "@/useStore";

export function App() {
  const { unauthorized } = useWorkbench();

  if (unauthorized) {
    return (
      <div className="mx-auto mt-[20vh] max-w-md px-6">
        <h1 className="text-xl font-semibold text-strong">Neko</h1>
        <p className="mt-2 text-muted-foreground">
          This page is not signed in to the daemon. Open Neko’s “Home (web)” page, or the address with{" "}
          <code className="rounded bg-foreground/5 px-1">?token=</code> from the <code className="rounded bg-foreground/5 px-1">web-token</code> file.
        </p>
      </div>
    );
  }
  return <AppShell />;
}
