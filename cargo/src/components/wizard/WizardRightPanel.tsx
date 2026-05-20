import { AlertTriangle, CheckCircle2, ShieldAlert, ShieldCheck, Sparkles } from 'lucide-react';
import { useWizard } from './WizardContext';
import { PolicyEditorBootstrap } from './policyTypes';
import { WIZARD_STEPS, WizardState } from './types';
import { evaluateWizardReadiness } from './wizardReadiness';

interface WizardRightPanelProps {
  state: WizardState;
  policyEditorBootstrap: PolicyEditorBootstrap | null;
}

export function WizardRightPanel({ state, policyEditorBootstrap }: WizardRightPanelProps) {
  const { policyEditorLoading, policyEditorError } = useWizard();
  const readiness = evaluateWizardReadiness(state, {
    policyEditorBootstrap,
    policyEditorLoading,
    policyEditorError,
  });
  const activeStep = WIZARD_STEPS[state.currentStep];
  const totalApps =
    state.apps.copiedItems.length
    + state.apps.wingetPackages.length
    + state.apps.chocolateyPackages.length
    + state.apps.customInstallers.length;

  return (
    <aside className="wizard-panel">
      <div className="wizard-panel-head">
        <h2 className="wizard-title text-lg">Live Status</h2>
        <p className="wizard-subtitle">{activeStep.title}</p>
      </div>

      <div className="wizard-panel-body space-y-4">
        <div className="rounded-xl border border-[var(--wiz-border)] bg-[var(--wiz-surface-muted)] p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs uppercase tracking-wide text-gray-500">Build Readiness</span>
            <span className="wizard-chip">{Math.round(((state.currentStep + 1) / WIZARD_STEPS.length) * 100)}%</span>
          </div>

          {readiness.canStartBuild ? (
            <div className="wizard-alert wizard-alert-success flex items-center gap-2">
              <CheckCircle2 size={15} />
              Ready to start build
            </div>
          ) : (
            <div className="wizard-alert wizard-alert-warning flex items-center gap-2">
              <ShieldAlert size={15} />
              Resolve blocking items before build
            </div>
          )}
        </div>

        {readiness.blockingErrors.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">Blocking Errors</p>
            {readiness.blockingErrors.map((issue) => (
              <div key={issue.code} className="wizard-alert wizard-alert-error flex items-start gap-2">
                <AlertTriangle size={15} className="mt-[1px] shrink-0" />
                <span>{issue.message}</span>
              </div>
            ))}
          </div>
        )}

        {readiness.warnings.length > 0 && (
          <div className="space-y-2">
            <p className="text-xs font-semibold uppercase tracking-wide text-gray-500">Warnings</p>
            {readiness.warnings.map((issue) => (
              <div key={issue.code} className="wizard-alert wizard-alert-warning flex items-start gap-2">
                <AlertTriangle size={15} className="mt-[1px] shrink-0" />
                <span>{issue.message}</span>
              </div>
            ))}
          </div>
        )}

        <div className="rounded-xl border border-[var(--wiz-border)] bg-[var(--wiz-surface-muted)] p-3">
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <Sparkles size={14} />
            Active Config
          </div>
          <div className="grid grid-cols-1 gap-2 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Windows</span>
              <span className="wizard-wrap-anywhere text-right font-medium text-gray-900">
                {state.windowsVersion.name} {state.windowsVersion.build}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Source</span>
              <span className="wizard-wrap-anywhere font-medium text-gray-900">
                {state.windowsVersion.sourceType === 'cloud'
                  ? 'Cloud'
                  : state.windowsVersion.sourcePath
                  ? 'Local file'
                  : 'Not set'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Output</span>
              <span className="wizard-wrap-anywhere font-medium text-gray-900">{state.output.outputType}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Users</span>
              <span className="font-medium text-gray-900">{state.userAccounts.length}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Applications</span>
              <span className="font-medium text-gray-900">{totalApps}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Scripts</span>
              <span className="font-medium text-gray-900">{readiness.configuredCustomScriptsCount}</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-gray-500">Policies</span>
              <span className="font-medium text-gray-900">
                {readiness.selectedPolicyCount + readiness.customRegistryEntryCount}
              </span>
            </div>
          </div>
        </div>

        <div className="rounded-xl border border-[var(--wiz-border)] bg-[var(--wiz-surface-muted)] p-3">
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-gray-500">
            <ShieldCheck size={14} />
            Current Step
          </div>
          <p className="text-sm font-semibold text-gray-900">{activeStep.title}</p>
          <p className="text-xs text-gray-500">{activeStep.description}</p>
        </div>
      </div>
    </aside>
  );
}
