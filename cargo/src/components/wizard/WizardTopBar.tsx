import { ArrowLeft, RefreshCcw } from 'lucide-react';
import { WIZARD_STEPS } from './types';

interface WizardTopBarProps {
  currentStep: number;
  onReset: () => void;
  onExit?: () => void;
}

export function WizardTopBar({ currentStep, onReset, onExit }: WizardTopBarProps) {
  return (
    <header className="wizard-topbar">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-[260px]">
          <h1 className="wizard-title text-[1.65rem]">BitOSDT Deployment Wizard</h1>
          <p className="wizard-subtitle">
            Step {currentStep + 1} of {WIZARD_STEPS.length} • {WIZARD_STEPS[currentStep].title}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <button type="button" onClick={onReset} className="wizard-btn wizard-btn-ghost inline-flex items-center gap-2">
            <RefreshCcw size={14} />
            Reset
          </button>
          {onExit && (
            <button type="button" onClick={onExit} className="wizard-btn wizard-btn-secondary inline-flex items-center gap-2">
              <ArrowLeft size={14} />
              Exit Wizard
            </button>
          )}
        </div>
      </div>
    </header>
  );
}
