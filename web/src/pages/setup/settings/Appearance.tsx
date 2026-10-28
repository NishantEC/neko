// Settings, Appearance: the mode (System, Light, Dark), the theme used in light
// and the theme used in dark, and the person's own theme files. The state and
// the daemon calls live in src/theme.ts; the themes are data (src/themes).
import { useState } from "react";
import { Button } from "@/ui/button";
import { CircleHalf, Moon, Palette, Sparkle, Sun, Warning } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { ConfirmDialog, Panel, Section, SettingRow } from "@/setup/kit";
import { removeUserTheme, setMode, setSlot, useThemes, type Mode, type ThemeInfo } from "@/theme";
import { ThemePicker } from "@/themes/ThemePicker";
import { ThemeDialog } from "@/themes/ThemeDialog";

const MODES: [Mode, string, typeof Sun][] = [["system", "System", CircleHalf], ["light", "Light", Sun], ["dark", "Dark", Moon]];

export function AppearanceSection() {
  const { choice, themes, problems, themesDir, active, appearance } = useThemes();
  const [making, setMaking] = useState(false);
  const [removing, setRemoving] = useState<ThemeInfo | null>(null);
  const light = themes.filter((t) => t.appearance === "light");
  const dark = themes.filter((t) => t.appearance === "dark");
  const mine = themes.filter((t) => t.source === "user");
  const inUse = (slot: "light" | "dark") =>
    choice.mode === "system" ? (appearance === slot ? `In use now, and whenever the Mac is ${slot}.` : slot === "light" ? "Used whenever the Mac is light." : "Used whenever the Mac is dark.")
    : choice.mode === slot ? "In use now." : `Used when the mode is System and the Mac is ${slot}, or when the mode is ${slot === "light" ? "Light" : "Dark"}.`;

  async function remove(theme: ThemeInfo) {
    try {
      await removeUserTheme(theme.id);
      toast.success(`Removed ${theme.name}`, { description: theme.overrides ? "The built-in came back." : undefined });
    } catch (e) {
      toast.error("Could not remove it", { description: e instanceof Error ? e.message : String(e) });
    }
  }

  return (
    <div className="flex flex-col gap-8" data-slot="appearance">
      <Section
        title="Appearance"
        intro="Pick one theme for when the screen is light and one for when it is dark. System follows your Mac and switches between the two. This changes the web app; the native app has its own switch."
      >
        <Panel>
          <SettingRow id="appearance" title="Mode" description={`Showing ${active.name} (${appearance}).`}>
            <div role="radiogroup" aria-label="Mode" className="flex gap-1.5">
              {MODES.map(([value, label, Icon]) => (
                <Button key={value} role="radio" aria-checked={choice.mode === value} data-mode={value} variant={choice.mode === value ? "secondary" : "outline"} size="sm" onClick={() => setMode(value)}>
                  <Icon /> {label}
                </Button>
              ))}
            </div>
          </SettingRow>
        </Panel>
      </Section>

      <Section id="setting-light-theme" title="Light theme" intro={inUse("light")}>
        <ThemePicker label="Light theme" themes={light} value={choice.light} onChange={(id) => setSlot("light", id)} onRemove={setRemoving} />
      </Section>

      <Section id="setting-dark-theme" title="Dark theme" intro={inUse("dark")}>
        <ThemePicker label="Dark theme" themes={dark} value={choice.dark} onChange={(id) => setSlot("dark", id)} onRemove={setRemoving} />
      </Section>

      <Section
        title="Your themes"
        intro={
          <>
            A theme is one small file in <code className="font-mono text-[12px]" data-slot="themes-dir">{themesDir ?? "<data dir>/themes"}</code>. Ask Neko to write one, or add a file yourself; it shows up here when it is saved. A file with a built-in’s id replaces it until you remove the file.
          </>
        }
        actions={<Button size="sm" data-slot="make-theme" onClick={() => setMaking(true)}><Sparkle /> Make me a theme</Button>}
      >
        <Panel>
          <div className="px-6 py-3 text-sm" data-slot="your-themes">
            {mine.length ? (
              <p className="flex items-center gap-2 text-muted-foreground"><Palette size={15} /> {mine.length === 1 ? "1 theme of yours" : `${mine.length} themes of yours`}, marked “Yours” above. Hover a card to remove it.</p>
            ) : (
              <p className="text-muted-foreground">None yet.</p>
            )}
          </div>
          {problems.length ? (
            <ul data-slot="theme-problems" className="flex flex-col gap-2 border-t border-border/70 px-6 py-3">
              {problems.map((p) => (
                <li key={p.file} role="alert" className="flex items-start gap-2 text-[13px]">
                  <Warning size={15} className="mt-0.5 shrink-0 text-warning-foreground" />
                  <span><code className="font-mono text-[12px]">{p.file}</code> was skipped: <span className="text-muted-foreground">{p.message}</span></span>
                </li>
              ))}
            </ul>
          ) : null}
        </Panel>
      </Section>

      <ThemeDialog open={making} onOpenChange={setMaking} dir={themesDir} />
      <ConfirmDialog
        open={removing !== null}
        onOpenChange={(open) => { if (!open) setRemoving(null); }}
        title={removing ? `Remove ${removing.name}?` : "Remove theme?"}
        description={removing ? <>Deletes <code className="font-mono text-[12px]">{removing.file}</code> from your themes folder. {removing.overrides ? "The built-in theme with this name comes back." : "You can ask Neko to make it again, but this cannot be undone."}</> : ""}
        confirmLabel="Remove"
        onConfirm={() => { const theme = removing; setRemoving(null); if (theme) void remove(theme); }}
      />
    </div>
  );
}
