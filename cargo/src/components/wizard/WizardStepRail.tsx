import { Check } from 'lucide-react';
import { WIZARD_STEPS } from './types';

interface WizardStepRailProps {
  currentStep: number;
  onSelectStep: (stepIndex: number) => void;
}

export function WizardStepRail({ currentStep, onSelectStep }: WizardStepRailProps) {
  const completionPercent = Math.round(((currentStep + 1) / WIZARD_STEPS.length) * 100);

  return (
    <aside className="wizard-panel">
      <div className="wizard-panel-head">
        <h2 className="wizard-title text-lg">Mission Steps</h2>
        <p className="wizard-subtitle">{completionPercent}% complete</p>
      </div>

      <div className="wizard-panel-body wizard-step-rail-body">
        <div className="space-y-2">
          {WIZARD_STEPS.map((step, index) => {
            const state = index < currentStep ? 'complete' : index === currentStep ? 'active' : 'locked';
            const clickable = index <= currentStep;

            return (
              <button
                key={step.id}
                type="button"
                onClick={() => clickable && onSelectStep(index)}
                data-state={state}
                data-clickable={clickable}
                className="wizard-step-item"
              >
                <div className="flex items-start gap-3">
                  <span data-state={state} className="wizard-step-node">
                    {state === 'complete' ? <Check size={14} /> : index + 1}
                  </span>
                  <span className="min-w-0">
                    <span className="block text-sm font-semibold text-gray-900">{step.title}</span>
                    <span className="mt-0.5 block text-xs text-gray-500">{step.description}</span>
                  </span>
                </div>
              </button>
            );
          })}
        </div>
      </div>
    </aside>
  );
}
