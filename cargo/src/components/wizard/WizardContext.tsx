import { invoke } from '@tauri-apps/api/tauri';
import { createContext, useContext, useEffect, useReducer, useState, ReactNode } from 'react';
import { WizardState, defaultWizardState, WIZARD_STEPS } from './types';
import { GroupPolicyState, PolicyEditorBootstrap } from './policyTypes';

type WizardAction =
  | { type: 'SET_STEP'; step: number }
  | { type: 'NEXT_STEP' }
  | { type: 'PREV_STEP' }
  | { type: 'UPDATE_WINDOWS_VERSION'; payload: Partial<WizardState['windowsVersion']> }
  | { type: 'UPDATE_OOBE'; payload: Partial<WizardState['oobeConfig']> }
  | { type: 'ADD_USER'; payload: WizardState['userAccounts'][0] }
  | { type: 'REMOVE_USER'; index: number }
  | { type: 'UPDATE_DOMAIN'; payload: Partial<WizardState['domainJoin']> }
  | { type: 'UPDATE_AUTOPILOT'; payload: Partial<WizardState['autopilot']> }
  | { type: 'UPDATE_APPS'; payload: Partial<WizardState['apps']> }
  | { type: 'UPDATE_WINDOWS_UPDATE'; payload: Partial<WizardState['windowsUpdate']> }
  | { type: 'UPDATE_GROUP_POLICIES'; payload: Partial<GroupPolicyState> }
  | { type: 'UPDATE_SHELL_LAYOUT'; payload: Partial<WizardState['shellLayout']> }
  | { type: 'UPDATE_OUTPUT'; payload: Partial<WizardState['output']> }
  | { type: 'REPLACE_STATE'; payload: WizardState }
  | { type: 'RESET' };

function wizardReducer(state: WizardState, action: WizardAction): WizardState {
  switch (action.type) {
    case 'SET_STEP':
      return { ...state, currentStep: action.step };
    case 'NEXT_STEP':
      return {
        ...state,
        currentStep: Math.min(state.currentStep + 1, WIZARD_STEPS.length - 1),
      };
    case 'PREV_STEP':
      return {
        ...state,
        currentStep: Math.max(state.currentStep - 1, 0),
      };
    case 'UPDATE_WINDOWS_VERSION':
      return {
        ...state,
        windowsVersion: { ...state.windowsVersion, ...action.payload },
      };
    case 'UPDATE_OOBE':
      return {
        ...state,
        oobeConfig: { ...state.oobeConfig, ...action.payload },
      };
    case 'ADD_USER':
      return {
        ...state,
        userAccounts: [...state.userAccounts, action.payload],
      };
    case 'REMOVE_USER':
      return {
        ...state,
        userAccounts: state.userAccounts.filter((_, i) => i !== action.index),
      };
    case 'UPDATE_DOMAIN':
      if (action.payload.promptForDomainCredentialsAtRuntime) {
        return {
          ...state,
          domainJoin: {
            ...state.domainJoin,
            ...action.payload,
            username: '',
            password: '',
          },
        };
      }
      return {
        ...state,
        domainJoin: { ...state.domainJoin, ...action.payload },
      };
    case 'UPDATE_AUTOPILOT':
      return {
        ...state,
        autopilot: { ...state.autopilot, ...action.payload },
      };
    case 'UPDATE_APPS':
      return {
        ...state,
        apps: { ...state.apps, ...action.payload },
      };
    case 'UPDATE_WINDOWS_UPDATE':
      return {
        ...state,
        windowsUpdate: { ...state.windowsUpdate, ...action.payload },
      };
    case 'UPDATE_GROUP_POLICIES':
      return {
        ...state,
        groupPolicies: { ...state.groupPolicies, ...action.payload },
      };
    case 'UPDATE_SHELL_LAYOUT':
      return {
        ...state,
        shellLayout: { ...state.shellLayout, ...action.payload },
      };
    case 'UPDATE_OUTPUT':
      if (action.payload.promptUncCredentialsAtRuntime) {
        return {
          ...state,
          output: {
            ...state.output,
            ...action.payload,
            fullIsoUncUsername: '',
            fullIsoUncPassword: '',
          },
        };
      }
      return {
        ...state,
        output: { ...state.output, ...action.payload },
      };
    case 'REPLACE_STATE':
      return action.payload;
    case 'RESET':
      return defaultWizardState;
    default:
      return state;
  }
}

interface WizardContextType {
  state: WizardState;
  dispatch: React.Dispatch<WizardAction>;
  editingImageId: string | null;
  legacyDefaultsWarning: string | null;
  policyEditorBootstrap: PolicyEditorBootstrap | null;
  policyEditorLoading: boolean;
  policyEditorError: string | null;
  reloadPolicyEditorBootstrap: (forceRefresh?: boolean) => Promise<void>;
}

const WizardContext = createContext<WizardContextType | null>(null);

export function WizardProvider({
  children,
  initialState,
  editingImageId,
  legacyDefaultsWarning,
}: {
  children: ReactNode;
  initialState?: WizardState;
  editingImageId?: string | null;
  legacyDefaultsWarning?: string | null;
}) {
  const [state, dispatch] = useReducer(wizardReducer, initialState ?? defaultWizardState);
  const [policyEditorBootstrap, setPolicyEditorBootstrap] = useState<PolicyEditorBootstrap | null>(null);
  const [policyEditorLoading, setPolicyEditorLoading] = useState(false);
  const [policyEditorError, setPolicyEditorError] = useState<string | null>(null);

  const reloadPolicyEditorBootstrap = async (forceRefresh = true) => {
    setPolicyEditorLoading(true);
    setPolicyEditorError(null);
    try {
      const bootstrap = await invoke<PolicyEditorBootstrap>('get_policy_editor_bootstrap', { forceRefresh });
      setPolicyEditorBootstrap(bootstrap);
    } catch (error) {
      console.error('Failed to load policy editor bootstrap:', error);
      setPolicyEditorBootstrap(null);
      setPolicyEditorError(String(error));
    } finally {
      setPolicyEditorLoading(false);
    }
  };

  useEffect(() => {
    void reloadPolicyEditorBootstrap(false);
  }, []);

  useEffect(() => {
    dispatch({ type: 'REPLACE_STATE', payload: initialState ?? defaultWizardState });
  }, [initialState]);

  return (
    <WizardContext.Provider
      value={{
        state,
        dispatch,
        editingImageId: editingImageId ?? null,
        legacyDefaultsWarning: legacyDefaultsWarning ?? null,
        policyEditorBootstrap,
        policyEditorLoading,
        policyEditorError,
        reloadPolicyEditorBootstrap,
      }}
    >
      {children}
    </WizardContext.Provider>
  );
}

export function useWizard() {
  const context = useContext(WizardContext);
  if (!context) {
    throw new Error('useWizard must be used within a WizardProvider');
  }
  return context;
}
