// Anything can ask for the first-run tour ("/setup", "/permissions", ⌘K):
// the Onboarding host listens and opens at the step asked for.
export type OnboardingStep = "welcome" | "permissions" | "workspace" | "runtime" | "import" | "finish";

type Listener = (step?: OnboardingStep) => void;
const listeners = new Set<Listener>();

export function openOnboarding(step?: OnboardingStep) {
  listeners.forEach((l) => l(step));
}

export function onOpenOnboarding(listener: Listener) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}
