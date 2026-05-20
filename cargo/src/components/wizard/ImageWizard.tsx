import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { WizardProvider, useWizard } from './WizardContext';
import { WIZARD_STEPS, WizardState, defaultWizardState } from './types';
import { StepWindowsSource } from './StepWindowsSource';
import { StepOobeUsers } from './StepOobeUsers';
import { StepDomainAutopilot } from './StepDomainAutopilot';
import { StepApplications } from './StepApplications';
import { StepWindowsUpdate } from './StepWindowsUpdate';
import { StepPolicies } from './StepPolicies';
import { StepOutput } from './StepOutput';
import { WizardTopBar } from './WizardTopBar';
import { WizardStepRail } from './WizardStepRail';
import { WizardRightPanel } from './WizardRightPanel';
import { normalizeWdsPxeOutput } from './wdsRuntimeSource';

interface WizardContentProps {
  onExit?: () => void;
}

interface ImageWizardProps {
  onExit?: () => void;
  editingImageId?: string | null;
  initialEditPayload?: ImageEditPayload | null;
}

interface ImageEditPayload {
  wizardState: unknown;
  legacyDefaultsApplied: boolean;
  legacyWarning?: string | null;
}

function coerceWizardState(raw: unknown): WizardState {
  const candidate = (raw && typeof raw === 'object' ? raw : {}) as Partial<WizardState>;
  const normalizedOutput = normalizeWdsPxeOutput({
    ...defaultWizardState.output,
    ...(candidate.output ?? {}),
  });

  return {
    ...defaultWizardState,
    ...candidate,
    windowsVersion: { ...defaultWizardState.windowsVersion, ...(candidate.windowsVersion ?? {}) },
    oobeConfig: { ...defaultWizardState.oobeConfig, ...(candidate.oobeConfig ?? {}) },
    userAccounts: Array.isArray(candidate.userAccounts) ? candidate.userAccounts : defaultWizardState.userAccounts,
    domainJoin: { ...defaultWizardState.domainJoin, ...(candidate.domainJoin ?? {}) },
    autopilot: { ...defaultWizardState.autopilot, ...(candidate.autopilot ?? {}) },
    apps: { ...defaultWizardState.apps, ...(candidate.apps ?? {}) },
    windowsUpdate: { ...defaultWizardState.windowsUpdate, ...(candidate.windowsUpdate ?? {}) },
    groupPolicies: { ...defaultWizardState.groupPolicies, ...(candidate.groupPolicies ?? {}) },
    shellLayout: { ...defaultWizardState.shellLayout, ...(candidate.shellLayout ?? {}) },
    output: normalizedOutput,
  };
}

function WizardContent({ onExit }: WizardContentProps) {
  const { state, dispatch, policyEditorBootstrap } = useWizard();
  const { currentStep } = state;
  const canGoPrev = currentStep > 0;
  const canGoNext = currentStep < WIZARD_STEPS.length - 1;

  const renderStep = () => {
    switch (currentStep) {
      case 0:
        return <StepWindowsSource />;
      case 1:
        return <StepOobeUsers />;
      case 2:
        return <StepDomainAutopilot />;
      case 3:
        return <StepApplications />;
      case 4:
        return <StepWindowsUpdate />;
      case 5:
        return <StepPolicies />;
      case 6:
        return <StepOutput />;
      default:
        return <StepWindowsSource />;
    }
  };

  return (
    <div className="wizard-shell">
      <div className="wizard-theme-scope">
        <WizardTopBar
          currentStep={currentStep}
          onReset={() => dispatch({ type: 'RESET' })}
          onExit={onExit}
        />

        <div className="wizard-grid">
          <WizardStepRail
            currentStep={currentStep}
            onSelectStep={(stepIndex) => dispatch({ type: 'SET_STEP', step: stepIndex })}
          />

          <div className="wizard-main-column min-w-0">
            <main className="wizard-main-card">{renderStep()}</main>

            <nav className="wizard-nav">
              <button
                type="button"
                onClick={() => dispatch({ type: 'PREV_STEP' })}
                disabled={!canGoPrev}
                className="wizard-btn wizard-btn-secondary"
              >
                Previous
              </button>

              <div className="text-center text-sm text-gray-500">
                Step {currentStep + 1} of {WIZARD_STEPS.length}
                <div className="font-semibold text-gray-900">{WIZARD_STEPS[currentStep].title}</div>
              </div>

              {canGoNext ? (
                <button
                  type="button"
                  onClick={() => dispatch({ type: 'NEXT_STEP' })}
                  className="wizard-btn wizard-btn-primary"
                >
                  Next
                </button>
              ) : (
                <button type="button" disabled className="wizard-btn wizard-btn-ghost">
                  Final Step
                </button>
              )}
            </nav>
          </div>

          <WizardRightPanel state={state} policyEditorBootstrap={policyEditorBootstrap} />
        </div>
      </div>
    </div>
  );
}

export function ImageWizard({ onExit, editingImageId = null, initialEditPayload = null }: ImageWizardProps) {
  const [initialState, setInitialState] = useState<WizardState>(() => {
    if (initialEditPayload?.wizardState) {
      return coerceWizardState(initialEditPayload.wizardState);
    }
    return defaultWizardState;
  });
  const [legacyWarning, setLegacyWarning] = useState<string | null>(initialEditPayload?.legacyWarning ?? null);
  const [loadingEditState, setLoadingEditState] = useState(() => !!editingImageId && !initialEditPayload);
  const [loadingEditStateSlow, setLoadingEditStateSlow] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let slowLoadTimer: number | null = null;
    const loadEditState = async () => {
      if (!editingImageId) {
        setInitialState(defaultWizardState);
        setLegacyWarning(null);
        setLoadingEditState(false);
        setLoadingEditStateSlow(false);
        return;
      }

      if (initialEditPayload) {
        setInitialState(coerceWizardState(initialEditPayload.wizardState));
        setLegacyWarning(initialEditPayload.legacyWarning ?? null);
        setLoadingEditState(false);
        setLoadingEditStateSlow(false);
        return;
      }

      setLoadingEditState(true);
      setLoadingEditStateSlow(false);
      slowLoadTimer = window.setTimeout(() => {
        if (!cancelled) {
          setLoadingEditStateSlow(true);
        }
      }, 8000);

      try {
        const payload = await invoke<ImageEditPayload>('get_image_edit_payload', {
          imageId: editingImageId,
        });
        if (cancelled) {
          return;
        }
        setInitialState(coerceWizardState(payload.wizardState));
        setLegacyWarning(payload.legacyWarning ?? null);
      } catch (error) {
        if (!cancelled) {
          console.error('Failed to load image edit payload:', error);
          setInitialState(defaultWizardState);
          setLegacyWarning('Failed to load the saved image profile. Default wizard settings were loaded.');
        }
      } finally {
        if (slowLoadTimer !== null) {
          window.clearTimeout(slowLoadTimer);
        }
        if (!cancelled) {
          setLoadingEditState(false);
          setLoadingEditStateSlow(false);
        }
      }
    };

    void loadEditState();
    return () => {
      cancelled = true;
      if (slowLoadTimer !== null) {
        window.clearTimeout(slowLoadTimer);
      }
    };
  }, [editingImageId, initialEditPayload]);

  if (loadingEditState) {
    return (
      <div className="ops-loading-screen">
        <div className="ops-spinner" />
        <p>Loading image profile...</p>
        {loadingEditStateSlow && (
          <div className="mt-4 max-w-md rounded-lg border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-900">
            <p className="font-medium">This is taking longer than expected.</p>
            <p className="mt-1">
              BitOSDT is still waiting for the saved profile payload. If this does not finish, exit the wizard and reopen the image.
            </p>
          </div>
        )}
        {onExit && (
          <button type="button" onClick={onExit} className="mt-4 rounded-lg border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-50">
            Exit Wizard
          </button>
        )}
      </div>
    );
  }

  return (
    <WizardProvider
      initialState={initialState}
      editingImageId={editingImageId}
      legacyDefaultsWarning={legacyWarning}
    >
      <WizardContent onExit={onExit} />
    </WizardProvider>
  );
}

export default ImageWizard;
