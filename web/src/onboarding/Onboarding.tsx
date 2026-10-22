// First run, at parity with OnboardingView.swift. Shown when the daemon says
// onboarding is not complete (GetOnboardingState) and when something asks for
// the tour again ("/setup", "/permissions", ⌘K "Show the first-run tour").
//
//   welcome -> permissions -> workspace -> runtime -> import (optional) -> done
//
// Skippable from every step after the first ("Skip setup" marks it complete,
// exactly like the native app's). ↵ continues, ⎋ goes back, Tab stays inside.
import { useCallback, useEffect, useRef, useState } from "react";
import { accept, command, request } from "@/api";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/ui/dialog";
import { Button } from "@/ui/button";
import { ArrowLeft, ArrowRight, CircleNotch } from "@/ui/icons";
import { toast } from "@/ui/sonner";
import { cn } from "@/ui/lib/utils";
import { onOpenOnboarding, type OnboardingStep } from "@/onboarding/open";
import { Finish, ImportStep, Permissions, Runtime, STEPS, Welcome, Workspace, type RuntimeChoice } from "@/onboarding/steps";

export function OnboardingHost() {
  const [open, setOpen] = useState(false);
  const [manual, setManual] = useState(false);
  const [step, setStep] = useState(0);
  const [choice, setChoice] = useState<RuntimeChoice | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const heading = useRef<HTMLDivElement>(null);

  // First run: ask the daemon once.
  useEffect(() => {
    let cancelled = false;
    request("GetOnboardingState")
      .then((r) => {
        if (!cancelled && r?.OnboardingState?.completed === false) setOpen(true);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // "/setup", "/permissions", the palette.
  useEffect(
    () =>
      onOpenOnboarding((at?: OnboardingStep) => {
        const index = Math.max(0, STEPS.findIndex((s) => s.id === at));
        setStep(index);
        setManual(true);
        setError(null);
        setOpen(true);
      }),
    [],
  );

  // Each step starts at its heading, so a screen reader reads it and Tab goes on from there.
  useEffect(() => {
    if (open) setTimeout(() => heading.current?.focus(), 50);
  }, [step, open]);

  const close = useCallback(() => {
    setOpen(false);
    setManual(false);
    setStep(0);
    setChoice(null);
    setError(null);
  }, []);

  const complete = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const reply = await request({ SetOnboardingComplete: { completed: true } });
      if (reply?.OnboardingState?.completed !== true) throw new Error("The daemon returned an unexpected response. Please try again.");
      close();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
    setBusy(false);
  }, [close]);

  const last = step === STEPS.length - 1;
  const next = useCallback(async () => {
    if (STEPS[step].id === "runtime" && choice) {
      setBusy(true);
      setError(null);
      try {
        accept(await command("SetAgentRuntime", { runtime: { provider: choice.provider, model: choice.model, reasoning_effort: null, service_tier: null } }));
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setBusy(false);
        return;
      }
      setBusy(false);
    }
    if (last) {
      await complete();
      toast("Neko is ready.");
    } else setStep((s) => s + 1);
  }, [step, choice, last, complete]);

  const back = useCallback(() => {
    setError(null);
    setStep((s) => Math.max(0, s - 1));
  }, []);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (next) return;
        // Esc: a step back; from the first step, only a re-opened tour closes (the first run is left with Skip setup).
        if (step > 0) back();
        else if (manual) close();
      }}
      disablePointerDismissal
    >
      <DialogContent
        data-slot="onboarding"
        showCloseButton={false}
        aria-label="Set up Neko"
        onKeyDown={(e) => {
          const target = e.target as HTMLElement;
          const interactive = target.closest("input, textarea, select, button, a, [role='radio'], [role='switch']");
          if (e.key === "Enter" && !interactive && !busy && !e.nativeEvent.isComposing) {
            e.preventDefault();
            void next();
          }
        }}
        className="h-[min(680px,92vh)] max-w-[calc(100%-2rem)] grid-rows-[1fr_auto] gap-0 overflow-hidden p-0 sm:max-w-[760px]"
      >
        <DialogTitle className="sr-only">Set up Neko — step {step + 1} of {STEPS.length}: {STEPS[step].label}</DialogTitle>
        <DialogDescription className="sr-only">A short tour: permissions, a workspace, the agent runtime and an optional import. You can skip it.</DialogDescription>
        <div className="min-h-0 overflow-y-auto px-10 pt-9 pb-6" data-step={STEPS[step].id}>
          {STEPS[step].id === "welcome" && <Welcome headingRef={heading} onNext={() => void next()} />}
          {STEPS[step].id === "permissions" && <Permissions headingRef={heading} />}
          {STEPS[step].id === "workspace" && <Workspace headingRef={heading} />}
          {STEPS[step].id === "runtime" && <Runtime headingRef={heading} choice={choice} onChoice={setChoice} />}
          {STEPS[step].id === "import" && <ImportStep headingRef={heading} />}
          {STEPS[step].id === "finish" && <Finish headingRef={heading} runtime={choice} />}
          {error ? <p role="alert" data-slot="onboarding-error" className="mt-4 select-text text-[13px] text-destructive">{error}</p> : null}
        </div>
        <footer className="flex items-center gap-3 border-t border-border/70 bg-foreground/[0.02] px-6 py-3.5">
          <Button type="button" variant="ghost" onClick={back} disabled={step === 0 || busy} className={cn(step === 0 && "invisible")} aria-hidden={step === 0}>
            <ArrowLeft /> Back
          </Button>
          <ol aria-label="Progress" className="mx-auto flex items-center gap-2">
            {STEPS.map((s, i) => (
              <li key={s.id} aria-current={i === step ? "step" : undefined} title={s.label} className="flex items-center">
                <span className={cn("block h-1.5 rounded-full transition-all", i === step ? "w-6 bg-foreground" : i < step ? "w-1.5 bg-foreground/60" : "w-1.5 bg-foreground/20")} />
                <span className="sr-only">{s.label}{i === step ? " (current step)" : ""}</span>
              </li>
            ))}
          </ol>
          {step > 0 && !last ? (
            <Button type="button" variant="ghost" onClick={() => void complete()} disabled={busy}>Skip setup</Button>
          ) : null}
          {step > 0 ? (
            <Button type="button" onClick={() => void next()} disabled={busy} data-slot="onboarding-next">
              {busy ? <CircleNotch className="animate-spin" /> : null}
              {last ? "Open Neko" : "Continue"} {!last && !busy ? <ArrowRight /> : null}
            </Button>
          ) : null}
        </footer>
      </DialogContent>
    </Dialog>
  );
}
