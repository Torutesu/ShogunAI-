import { useState } from "react";
import type { JSX } from "react";
import { t } from "../../strings";

type Appearance = "auto" | "light" | "dark";

function storedAppearance(): Appearance {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem("shogun.appearance") ?? '"auto"');
    return value === "light" || value === "dark" ? value : "auto";
  } catch {
    return "auto";
  }
}

function storeAppearance(appearance: Appearance): void {
  try {
    window.localStorage.setItem("shogun.appearance", JSON.stringify(appearance));
  } catch {
    // WebKit can deny storage in a damaged profile. Keep setup usable; app safely defaults to System.
  }
}

export function ThemeStage({ onBack, onContinue }: { onBack: () => Promise<boolean>; onContinue: () => Promise<boolean> }): JSX.Element {
  const [appearance, setAppearance] = useState<Appearance>(storedAppearance);
  const choices: Array<{ id: Appearance; label: string }> = [
    { id: "auto", label: t.onboarding.themeSystem },
    { id: "light", label: t.onboarding.themeLight },
    { id: "dark", label: t.onboarding.themeDark },
  ];
  const select = (next: Appearance): void => {
    setAppearance(next);
    storeAppearance(next);
  };

  return (
    <section className="onb-stage onb-stage--theme">
      <div>
        <p className="onb-eyebrow">{t.onboarding.themeStep}</p>
        <h1>{t.onboarding.themeTitle}</h1>
        <p className="onb-lead">{t.onboarding.themeLead}</p>
        <div className="onb-theme-options" role="radiogroup" aria-label={t.onboarding.themeTitle}>
          {choices.map(({ id, label }) => (
            <button className="onb-theme-choice" type="button" role="radio" aria-checked={appearance === id} data-selected={appearance === id} key={id} onClick={() => select(id)}>
              <span className="onb-theme-preview" data-preview={id} aria-hidden="true">
                <span className="onb-theme-preview__lights"><i /><i /><i /></span>
                <span className="onb-theme-preview__sidebar"><i /><i /><i /></span>
                <span className="onb-theme-preview__canvas"><i /><i /><i /></span>
              </span>
              <span>{label}</span>
            </button>
          ))}
        </div>
      </div>
      <nav className="onb-overview__actions" aria-label={t.onboarding.overviewNavigation}>
        <button className="onb-button onb-button--back" type="button" onClick={() => void onBack()}>
          <span aria-hidden="true">←</span>{t.onboarding.back}
        </button>
        <button className="onb-button onb-button--next" type="button" onClick={() => void onContinue()}>
          {t.onboarding.next}<span aria-hidden="true">↵</span>
        </button>
      </nav>
    </section>
  );
}
