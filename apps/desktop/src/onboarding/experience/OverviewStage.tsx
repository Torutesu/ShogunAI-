import type { JSX } from "react";
import { t } from "../../strings";
import { IconAlignJustify, IconBrain, IconCopy, IconHistory } from "../../utilityIcons";

export function OverviewStage({ onBack, onContinue }: { onBack: () => Promise<boolean>; onContinue: () => Promise<boolean> }): JSX.Element {
  const capabilities = [
    { title: t.onboarding.overviewRewrite, icon: IconAlignJustify, tone: "ember" },
    { title: t.onboarding.overviewDictate, icon: IconCopy, tone: "glacier" },
    { title: t.onboarding.overviewRecall, icon: IconBrain, tone: "cedar" },
    { title: t.onboarding.overviewMeetings, icon: IconHistory, tone: "blue" },
  ] as const;

  return (
    <section className="onb-stage onb-stage--overview">
      <div>
        <p className="onb-eyebrow">{t.onboarding.overviewStep}</p>
        <h1>{t.onboarding.overviewTitle}</h1>
        <p className="onb-lead">{t.onboarding.overviewLead}</p>
        <ul className="onb-overview__list" aria-label={t.onboarding.overviewFeatures}>
          {capabilities.map(({ title, icon: Icon, tone }) => (
            <li key={title}>
              <span className="onb-overview__icon" data-tone={tone} aria-hidden="true"><Icon size={17} /></span>
              <span>{title}</span>
            </li>
          ))}
        </ul>
      </div>
      <nav className="onb-overview__actions" aria-label={t.onboarding.overviewNavigation}>
        <button className="onb-button onb-button--back" type="button" aria-keyshortcuts="Backspace" onClick={() => void onBack()}>
          <span aria-hidden="true">←</span>{t.onboarding.back}
        </button>
        <button className="onb-button onb-button--next" type="button" aria-keyshortcuts="Enter" onClick={() => void onContinue()}>
          {t.onboarding.next}<span aria-hidden="true">↵</span>
        </button>
      </nav>
    </section>
  );
}
