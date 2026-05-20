import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';
import { useWizard } from './WizardContext';
import { evaluateWizardReadiness } from './wizardReadiness';
import { LocalPayloadItem, deriveLocalPayloadDisplayName } from '../../types/localPayload';
import { LightweightHostPanel } from '../lightweight/LightweightHostPanel';
import type { SimpleDeliveryDefaults } from '../lightweight/lightweightHostTypes';
import { inferWdsRuntimeSource } from './wdsRuntimeSource';
import { AppModal } from '../shared/AppModal';
import { CredentialWarningDialog } from '../shared/CredentialWarningDialog';

function createDriverPayloadItem(sourcePath: string): LocalPayloadItem {
  return {
    sourcePath,
    sourceKind: 'Directory',
    displayName: sourcePath.split(/[\\/]/).filter(Boolean).pop() || sourcePath,
  };
}

function formatBuildStep(step: string): string {
  switch (step) {
    case 'init':
      return 'Initializing';
    case 'source':
      return 'Resolving source';
    case 'source-normalize':
      return 'Normalizing source';
    case 'download':
      return 'Downloading';
    case 'extract':
      return 'Extracting ISO';
    case 'convert':
      return 'Converting image';
    case 'prepare':
      return 'Preparing image';
    case 'prepare-copy':
      return 'Preparing image: copy';
    case 'prepare-mount':
      return 'Preparing image: mount';
    case 'prepare-unattend':
      return 'Preparing image: unattend';
    case 'prepare-autopilot':
      return 'Preparing image: Autopilot';
    case 'prepare-task-sequence':
      return 'Preparing image: task sequence';
    case 'prepare-files':
      return 'Preparing image: file injection';
    case 'prepare-drivers':
      return 'Preparing image: drivers';
    case 'prepare-remove-apps':
      return 'Preparing image: app cleanup';
    case 'prepare-enable-features':
      return 'Preparing image: enabling features';
    case 'prepare-disable-features':
      return 'Preparing image: disabling features';
    case 'prepare-commit':
      return 'Preparing image: commit';
    case 'prepare-discard':
      return 'Preparing image: discard';
    case 'prepare-complete':
      return 'Preparing image: complete';
    case 'winpe':
      return 'Building WinPE';
    case 'iso':
      return 'Creating ISO';
    case 'publish':
      return 'Publishing output';
    case 'host':
      return 'Starting host';
    case 'complete':
      return 'Complete';
    default:
      return step
        .split(/[-_]/)
        .filter(Boolean)
        .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
        .join(' ');
  }
}

type BuildWorkspaceRecoveryStatus = 'ok' | 'locked_with_matches' | 'locked_without_matches';

type BuildWorkspaceRecoveryProcess = {
  pid: number;
  executable: string;
  command_line: string;
};

type BuildWorkspaceRecoveryResponse = {
  status: BuildWorkspaceRecoveryStatus;
  message: string;
  locked_path: string | null;
  processes: BuildWorkspaceRecoveryProcess[];
};

function formatTauriError(error: unknown): string {
  if (typeof error === 'string') {
    return error;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

export function StepOutput() {
  const {
    state,
    dispatch,
    editingImageId,
    legacyDefaultsWarning,
    policyEditorBootstrap,
    policyEditorLoading,
    policyEditorError,
  } = useWizard();
  const { output, windowsVersion } = state;
  const isEditing = !!editingImageId;
  const [isBuilding, setIsBuilding] = useState(false);
  const [buildProgress, setBuildProgress] = useState<string[]>([]);
  const [buildProgressPercent, setBuildProgressPercent] = useState(0);
  const [currentStep, setCurrentStep] = useState('');
  const [showConfirm, setShowConfirm] = useState(false);
  const lastDownloadLineIndexRef = useRef<number | null>(null);
  const currentStepRef = useRef<string>('');
  const terminalRef = useRef<HTMLDivElement>(null);
  const [stallWarning, setStallWarning] = useState<string | null>(null);
  const [preflightError, setPreflightError] = useState<string | null>(null);
  const [saveMode, setSaveMode] = useState<'overwrite' | 'copy'>('overwrite');
  const [simpleDefaults, setSimpleDefaults] = useState<SimpleDeliveryDefaults | null>(null);
  const [hostPanelRefreshToken, setHostPanelRefreshToken] = useState(0);
  const [showCredentialWarning, setShowCredentialWarning] = useState(false);
  const [credentialWarningSuppressed, setCredentialWarningSuppressed] = useState(false);
  const [isCancelling, setIsCancelling] = useState(false);
  const [workspaceRecoveryPrompt, setWorkspaceRecoveryPrompt] = useState<BuildWorkspaceRecoveryResponse | null>(null);
  const [isRecoveringWorkspace, setIsRecoveringWorkspace] = useState(false);
  const [shouldScrollBuildIntoView, setShouldScrollBuildIntoView] = useState(false);
  const [shouldScrollCompletionIntoView, setShouldScrollCompletionIntoView] = useState(false);
  const stallTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const stallTimerGenerationRef = useRef(0);
  const getBaseName = (path: string) => path.split(/[\\/]/).pop() || path;
  const includesLightweightSelection =
    output.outputType === 'LightweightISO' || output.outputType === 'Both';
  const includesFullIsoSelection =
    output.outputType === 'FullISO' || output.outputType === 'Both';
  const isWdsPxeSelection = output.outputType === 'WDSPXE';
  const wdsExportRoot = 'C:\\BitOSDT\\WDS';
  const wdsRuntimeSource = inferWdsRuntimeSource(output);
  const configuredWdsRuntimePath =
    wdsRuntimeSource === 'HTTP'
      ? (output.fullIsoHttpUrl?.trim() || '')
      : (output.fullIsoUncPath?.trim() || '');
  const hasConfiguredUncAuth =
    !!output.fullIsoUncPath?.trim()
    && !!output.fullIsoUncUsername?.trim()
    && !!output.fullIsoUncPassword;
  const shouldShowWdsUncAuth = isWdsPxeSelection && wdsRuntimeSource === 'UNC';
  const shouldShowFullIsoUncAuth = !isWdsPxeSelection && !!output.fullIsoUncPath?.trim();
  const buildDriverPaths = [
    ...output.driverPaths.map((item) => item.sourcePath),
    ...(isWdsPxeSelection && output.bootDriverUncPath?.trim() ? [output.bootDriverUncPath.trim()] : []),
  ];

  useEffect(() => {
    invoke<boolean>('get_credential_warning_suppressed')
      .then((suppressed) => setCredentialWarningSuppressed(suppressed))
      .catch(() => setCredentialWarningSuppressed(false));
  }, []);

  const getStallConfig = (step: string): { timeout: number; message: string } => {
    if (step.startsWith('prepare-')) {
      switch (step) {
        case 'prepare-copy':
          return { timeout: 90000, message: '⏳ Copying the Windows image is still running - large WIM files can take a while' };
        case 'prepare-mount':
          return { timeout: 120000, message: '⏳ Mounting the Windows image is still running - DISM can take several minutes' };
        case 'prepare-unattend':
        case 'prepare-autopilot':
        case 'prepare-task-sequence':
        case 'prepare-files':
        case 'prepare-remove-apps':
        case 'prepare-enable-features':
        case 'prepare-disable-features':
          return { timeout: 120000, message: '⏳ Offline Windows customizations are still running - waiting for the next step' };
        case 'prepare-drivers':
          return { timeout: 180000, message: '⏳ Driver injection is still running - DISM can take several minutes for large driver sets' };
        case 'prepare-commit':
        case 'prepare-discard':
          return { timeout: 180000, message: '⏳ Finalizing Windows image changes is still running - DISM can take several minutes' };
        default:
          return { timeout: 120000, message: '⏳ Image preparation is still running - this can take several minutes' };
      }
    }

    switch (step) {
      case 'download':
        return { timeout: 30000, message: '⚠️ Download may be stalled - no progress for 30 seconds' };
      case 'convert':
        return { timeout: 120000, message: '⏳ ESD to WIM conversion is still running - this can take several minutes for large images' };
      case 'source-normalize':
        return { timeout: 120000, message: '⏳ Windows source normalization is still running - DISM can take several minutes for large images' };
      case 'prepare':
        return { timeout: 120000, message: '⏳ Image preparation is still running - this can take several minutes' };
      case 'winpe':
        return { timeout: 120000, message: '⏳ WinPE creation is still running - this can take a few minutes' };
      case 'iso':
        return { timeout: 120000, message: '⏳ ISO creation is still running - this can take several minutes for large images' };
      case 'extract':
        return { timeout: 90000, message: '⏳ ISO extraction is still running - this can take a few minutes' };
      case 'publish':
        return { timeout: 90000, message: '⏳ PXE/lightweight publish staging is still running - this can take a few minutes' };
      case 'export':
        return { timeout: 90000, message: '⏳ WDS/PXE artifact export is still running - waiting for the next progress update' };
      case 'host':
        return { timeout: 30000, message: '⏳ Embedded lightweight host startup is still running - waiting for a response' };
      default:
        return { timeout: 60000, message: '⏳ Operation is still running - no progress update for 60 seconds' };
    }
  };

  const startStallTimer = () => {
    if (stallTimerRef.current) clearTimeout(stallTimerRef.current);
    const { timeout, message } = getStallConfig(currentStepRef.current);
    const generation = ++stallTimerGenerationRef.current;
    stallTimerRef.current = setTimeout(() => {
      if (generation === stallTimerGenerationRef.current) {
        setStallWarning(message);
      }
    }, timeout);
  };

  const clearStallTimer = () => {
    stallTimerGenerationRef.current += 1;
    if (stallTimerRef.current) {
      clearTimeout(stallTimerRef.current);
      stallTimerRef.current = null;
    }
    setStallWarning(null);
  };

  useEffect(() => {
    let unlisten: (() => void) | null = null;

    const setupListener = async () => {
      startStallTimer();
      unlisten = await listen('build-progress', (event: any) => {
        const { step, progress, message } = event.payload;
        currentStepRef.current = step;
        setCurrentStep(step);
        clearStallTimer();
        startStallTimer();
        setBuildProgressPercent(progress);

        // For download step, update the last download line instead of appending
        if (step === 'download' && message.startsWith('Downloading:')) {
          setBuildProgress((prev) => {
            const newLine = `[${progress}%] ${message}`;
            if (lastDownloadLineIndexRef.current !== null && lastDownloadLineIndexRef.current < prev.length) {
              // Update the existing download progress line
              const updated = [...prev];
              updated[lastDownloadLineIndexRef.current] = newLine;
              return updated;
            } else {
              // First download progress line - append and track index
              lastDownloadLineIndexRef.current = prev.length;
              return [...prev, newLine];
            }
          });
        } else {
          // Non-download step or non-progress message - append normally
          lastDownloadLineIndexRef.current = null; // Reset for next download
          setBuildProgress((prev) => [...prev, `[${progress}%] ${message}`]);
        }
      });
    };

    if (isBuilding) {
      setupListener();
    } else {
      clearStallTimer();
    }

    return () => {
      if (unlisten) {
        unlisten();
      }
      clearStallTimer();
    };
  }, [isBuilding]);

  // Auto-scroll build log to bottom
  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
    }
  }, [buildProgress]);

  useEffect(() => {
    if (!shouldScrollBuildIntoView) {
      return;
    }

    const scrollTarget = window.setTimeout(() => {
      terminalRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      setShouldScrollBuildIntoView(false);
    }, 60);

    return () => window.clearTimeout(scrollTarget);
  }, [shouldScrollBuildIntoView, buildProgress.length, isBuilding]);

  useEffect(() => {
    if (!shouldScrollCompletionIntoView || isBuilding || buildProgress.length === 0) {
      return;
    }

    const scrollTarget = window.setTimeout(() => {
      terminalRef.current?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      setShouldScrollCompletionIntoView(false);
    }, 120);

    return () => window.clearTimeout(scrollTarget);
  }, [shouldScrollCompletionIntoView, isBuilding, buildProgress.length]);

  const handleBrowse = async () => {
    if (isWdsPxeSelection) {
      dispatch({ type: 'UPDATE_OUTPUT', payload: { outputPath: wdsExportRoot } });
      return;
    }

    try {
      const defaultFileName = `${windowsVersion.name.replace(/\s+/g, '')}-${windowsVersion.build}-${windowsVersion.edition}.iso`;
      const result = await invoke<string | null>('show_save_dialog', {
        defaultPath: defaultFileName,
        title: 'Save ISO Image',
      });

      if (result) {
        dispatch({ type: 'UPDATE_OUTPUT', payload: { outputPath: result } });
      }
    } catch (err) {
      console.error('Failed to open file dialog:', err);
      // Fallback to default path
      const defaultPath = `${windowsVersion.name.replace(/\s+/g, '')}-${windowsVersion.build}-${windowsVersion.edition}.iso`;
      dispatch({ type: 'UPDATE_OUTPUT', payload: { outputPath: defaultPath } });
    }
  };

  useEffect(() => {
    if (!isWdsPxeSelection) {
      return;
    }

    const payload: Partial<typeof output> = {};
    if (output.outputPath !== wdsExportRoot) {
      payload.outputPath = wdsExportRoot;
    }
    if (output.wdsRuntimeSource !== wdsRuntimeSource) {
      payload.wdsRuntimeSource = wdsRuntimeSource;
    }
    if (wdsRuntimeSource === 'UNC' && (output.fullIsoHttpUrl ?? '') !== '') {
      payload.fullIsoHttpUrl = '';
    }
    if (wdsRuntimeSource === 'HTTP' && (output.fullIsoUncPath ?? '') !== '') {
      payload.fullIsoUncPath = '';
    }

    if (Object.keys(payload).length > 0) {
      dispatch({ type: 'UPDATE_OUTPUT', payload });
    }
  }, [dispatch, isWdsPxeSelection, output, wdsRuntimeSource]);

  useEffect(() => {
    const shouldClearUncAuth =
      !output.fullIsoUncPath?.trim() || (isWdsPxeSelection && wdsRuntimeSource === 'HTTP');

    if (
      shouldClearUncAuth
      && ((output.fullIsoUncUsername ?? '') !== '' || (output.fullIsoUncPassword ?? '') !== '')
    ) {
      dispatch({
        type: 'UPDATE_OUTPUT',
        payload: {
          fullIsoUncUsername: '',
          fullIsoUncPassword: '',
        },
      });
    }
  }, [
    dispatch,
    isWdsPxeSelection,
    output.fullIsoUncPassword,
    output.fullIsoUncPath,
    output.fullIsoUncUsername,
    wdsRuntimeSource,
  ]);

  const handleOutputTypeChange = (nextOutputType: 'FullISO' | 'LightweightISO' | 'Both' | 'WDSPXE') => {
    const payload: Partial<typeof output> = { outputType: nextOutputType };
    if (nextOutputType === 'WDSPXE') {
      const nextRuntimeSource =
        output.fullIsoHttpUrl?.trim() && !output.fullIsoUncPath?.trim() ? 'HTTP' : 'UNC';
      payload.outputPath = wdsExportRoot;
      payload.wdsRuntimeSource = nextRuntimeSource;
      if (nextRuntimeSource === 'UNC') {
        payload.fullIsoHttpUrl = '';
      } else {
        payload.fullIsoUncPath = '';
        payload.fullIsoUncUsername = '';
        payload.fullIsoUncPassword = '';
      }
    }
    dispatch({
      type: 'UPDATE_OUTPUT',
      payload,
    });
  };

  const handleWdsRuntimeSourceChange = (nextRuntimeSource: 'UNC' | 'HTTP') => {
    dispatch({
      type: 'UPDATE_OUTPUT',
      payload: nextRuntimeSource === 'UNC'
        ? { wdsRuntimeSource: 'UNC', fullIsoHttpUrl: '' }
        : {
          wdsRuntimeSource: 'HTTP',
          fullIsoUncPath: '',
          fullIsoUncUsername: '',
          fullIsoUncPassword: '',
        },
    });
  };

  useEffect(() => {
    if (!includesLightweightSelection) {
      return;
    }

    let cancelled = false;
    const loadDeliveryDefaults = async () => {
      try {
        const defaults = await invoke<SimpleDeliveryDefaults>('get_simple_delivery_defaults');
        if (!cancelled) {
          setSimpleDefaults(defaults);
        }
      } catch (error) {
        console.error('Failed to load simple delivery defaults:', error);
      }
    };

    loadDeliveryDefaults();

    return () => {
      cancelled = true;
    };
  }, [includesLightweightSelection]);

  const handleBrowsePxeExportPath = async () => {
    try {
      const result = await invoke<string | null>('show_folder_dialog', {
        title: 'Select PXE/WDS Export Folder',
      });

      if (result) {
        dispatch({ type: 'UPDATE_OUTPUT', payload: { pxeExportPath: result } });
      }
    } catch (error) {
      console.error('Failed to open PXE export folder dialog:', error);
    }
  };

  const handleAddDriverFolder = async () => {
    try {
      const result = await invoke<string | null>('show_folder_dialog', {
        title: 'Select Boot Driver Folder',
      });

      if (!result) {
        return;
      }

      const trimmed = result.trim();
      if (!trimmed) {
        return;
      }

      if (output.driverPaths.some((item) => item.sourcePath.toLowerCase() === trimmed.toLowerCase())) {
        return;
      }

      dispatch({
        type: 'UPDATE_OUTPUT',
        payload: { driverPaths: [...output.driverPaths, createDriverPayloadItem(trimmed)] },
      });
    } catch (error) {
      console.error('Failed to open driver folder dialog:', error);
    }
  };

  const removeDriverFolder = (sourcePath: string) => {
    dispatch({
      type: 'UPDATE_OUTPUT',
      payload: {
        driverPaths: output.driverPaths.filter((item) => item.sourcePath !== sourcePath),
      },
    });
  };

  const readiness = evaluateWizardReadiness(state, {
    isBuilding,
    policyEditorBootstrap,
    policyEditorLoading,
    policyEditorError,
  });
  const hasValidSource = readiness.hasValidSource;
  const hasCustomInstallers = readiness.hasCustomInstallers;
  const hasLocalPayloadCopyWork = readiness.hasLocalPayloadCopyWork;
  const includesLightweightOutput = readiness.includesLightweightOutput;
  const hasBootDrivers = readiness.hasBootDrivers;
  const normalizedComputerName = readiness.normalizedComputerName;
  const computerNameValidationError = readiness.computerNameValidationError;
  const hasExplicitComputerName = readiness.hasExplicitComputerName;
  const configuredCustomScriptsCount = readiness.configuredCustomScriptsCount;
  const totalPolicySelections = readiness.selectedPolicyCount + readiness.customRegistryEntryCount;
  const lightweightCompatibilityErrors = readiness.blockingErrors
    .filter((issue) => issue.code.startsWith('lightweight-'))
    .map((issue) => issue.message);
  const lightweightWarnings = readiness.warnings
    .filter((issue) =>
      issue.code === 'lightweight-custom-installers' || issue.code === 'lightweight-local-payloads',
    )
    .map((issue) => issue.message);
  const wdsSignInWarnings = readiness.warnings
    .filter((issue) => issue.code.startsWith('wds-sign-in-'))
    .map((issue) => issue.message);
  const canStartBuild = readiness.canStartBuild;
  const hasPlaintextCredentials =
    hasConfiguredUncAuth || (!!state.domainJoin.enabled && !!state.domainJoin.password);

  const handleBuild = async () => {
    if (readiness.blockingErrors.length > 0) {
      setPreflightError(readiness.blockingErrors[0].message);
      return;
    }

    if (hasPlaintextCredentials && !credentialWarningSuppressed) {
      setShowCredentialWarning(true);
      return;
    }

    await executeBuild();
  };

  const executeBuild = async (skipWorkspaceRecoveryCheck = false) => {
    setPreflightError(null);
    setWorkspaceRecoveryPrompt(null);

    if (!skipWorkspaceRecoveryCheck) {
      try {
        const recovery = await invoke<BuildWorkspaceRecoveryResponse>('check_build_workspace_recovery');
        if (recovery.status === 'locked_with_matches') {
          setWorkspaceRecoveryPrompt(recovery);
          return;
        }
        if (recovery.status === 'locked_without_matches') {
          setPreflightError(recovery.message);
          return;
        }
      } catch (error) {
        setPreflightError(formatTauriError(error));
        return;
      }
    }

    setIsBuilding(true);
    setIsCancelling(false);
    setShouldScrollBuildIntoView(true);
    setShouldScrollCompletionIntoView(true);
    setBuildProgress(['Starting build process...']);
    setBuildProgressPercent(0);

    try {
      // Build the request payload - only send relevant source info based on source type
      const request = {
        windows_version: windowsVersion.name,
        windows_build: windowsVersion.build,
        windows_edition: windowsVersion.edition,
        windows_channel: windowsVersion.channel || null,
        language: windowsVersion.language || 'en-us',
        output_type: output.outputType,
        output_path: output.outputPath,
        volume_label: output.volumeLabel,
        source_path: windowsVersion.sourceType === 'local' ? (windowsVersion.sourcePath || null) : null,
        download_url: windowsVersion.sourceType === 'cloud' ? (windowsVersion.downloadUrl || null) : null,
        delivery_mode: output.deliveryMode,
        server_url: output.deliveryMode === 'Advanced' ? (output.serverUrl || null) : (simpleDefaults?.runtimeUrl || null),
        pxe_export_path: output.deliveryMode === 'Advanced' ? (output.pxeExportPath || null) : (simpleDefaults?.publishPath || null),
        full_iso_unc_path: output.fullIsoUncPath?.trim() ? output.fullIsoUncPath : null,
        full_iso_unc_username: output.fullIsoUncPath?.trim() && output.fullIsoUncUsername?.trim()
          ? output.fullIsoUncUsername.trim()
          : null,
        full_iso_unc_password: output.fullIsoUncPath?.trim() && output.fullIsoUncPassword
          ? output.fullIsoUncPassword
          : null,
        full_iso_http_url: output.fullIsoHttpUrl?.trim() ? output.fullIsoHttpUrl : null,
        prompt_unc_credentials_at_runtime: output.promptUncCredentialsAtRuntime || null,
        driver_paths: buildDriverPaths,
        boot_driver_unc_path: isWdsPxeSelection && output.bootDriverUncPath?.trim()
          ? output.bootDriverUncPath.trim()
          : null,
        apply_to_offline_windows: output.applyDriversToOfflineWindows,
        include_gui: output.includeGui,
        existing_image_id: isEditing ? editingImageId : null,
        save_mode: isEditing ? saveMode : null,
        oobe_config: state.oobeConfig,
        user_accounts: state.userAccounts,
        domain_join: state.domainJoin,
        autopilot: state.autopilot,
        apps: state.apps,
        windows_update: state.windowsUpdate,
        group_policies: state.groupPolicies,
        shell_layout: state.shellLayout,
      };

      setBuildProgress((prev) => [...prev, 'Sending build request to backend...']);

      // Call the actual Tauri command
      const result = await invoke<string>('build_image', { request });

      setBuildProgress((prev) => [...prev, `✓ Build completed: ${result}`]);
      setBuildProgressPercent(100);
      if (output.deliveryMode === 'Simple' && includesLightweightSelection) {
        setHostPanelRefreshToken((current) => current + 1);
      }
    } catch (error) {
      const message = formatTauriError(error);
      if (message.includes('Build cancelled by user')) {
        setBuildProgress((prev) => [...prev, '✗ Build cancelled by user']);
      } else {
        setBuildProgress((prev) => [...prev, `✗ Error: ${message}`]);
      }
    } finally {
      setIsCancelling(false);
      setIsBuilding(false);
    }
  };

  const handleCancel = async () => {
    if (isCancelling) {
      return;
    }

    setIsCancelling(true);
    setBuildProgress((prev) => [...prev, 'Requesting build cancellation...']);
    try {
      await invoke('cancel_build');
      setBuildProgress((prev) => [...prev, 'Cancellation acknowledged. Waiting for active tools to stop...']);
    } catch (error) {
      console.error('Failed to cancel build:', error);
      setIsCancelling(false);
      setBuildProgress((prev) => [...prev, `✗ Error: ${formatTauriError(error)}`]);
    }
  };

  const handleWorkspaceRecovery = async () => {
    setIsRecoveringWorkspace(true);
    setPreflightError(null);
    try {
      const recovery = await invoke<BuildWorkspaceRecoveryResponse>('recover_build_workspace');
      if (recovery.status === 'ok') {
        setWorkspaceRecoveryPrompt(null);
        await executeBuild(true);
        return;
      }

      setWorkspaceRecoveryPrompt(recovery);
      setPreflightError(recovery.message);
    } catch (error) {
      setPreflightError(formatTauriError(error));
    } finally {
      setIsRecoveringWorkspace(false);
    }
  };

  return (
    <div className="wizard-step space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-gray-900 mb-2">Output Configuration</h2>
        <p className="text-gray-600">Configure the output type and location for your deployment image.</p>
      </div>

      {legacyDefaultsWarning && (
        <div className="bg-amber-50 border border-amber-300 rounded-lg p-4">
          <p className="text-amber-800 text-sm font-medium">{legacyDefaultsWarning}</p>
        </div>
      )}

      {/* Output Type */}
      <div className="bg-gray-50 rounded-lg p-6">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Output Type</h3>
        <div className="space-y-4">
          {[
            {
              value: 'FullISO',
              label: 'Full ISO',
              description:
                'Complete bootable ISO with all customizations baked in. Ready for USB or PXE deployment.',
              size: '~6-8 GB',
            },
            {
              value: 'LightweightISO',
              label: 'Lightweight ISO',
              description:
                'Small WinPE-based ISO that downloads the image during deployment. Requires network access.',
              size: '~500 MB',
              experimental: true,
            },
            {
              value: 'Both',
              label: 'Both',
              description: 'Generate both full and lightweight ISOs.',
              size: '~7-8 GB total',
              experimental: true,
            },
            {
              value: 'WDSPXE',
              label: 'WDS / PXE',
              description:
                'Export a WDS-friendly boot.wim and Windows payload bundle for PXE boot. You host the final install image path.',
              size: 'boot.wim + payload',
            },
          ].map((option) => (
            <label
              key={option.value}
              className={`flex items-start p-4 rounded-lg border cursor-pointer transition-colors ${output.outputType === option.value
                  ? 'bg-blue-50 border-blue-500'
                  : 'bg-white border-gray-200 hover:border-blue-300'
                }`}
            >
              <input
                type="radio"
                name="outputType"
                checked={output.outputType === option.value}
                onChange={() => handleOutputTypeChange(option.value as 'FullISO' | 'LightweightISO' | 'Both' | 'WDSPXE')}
                className="w-5 h-5 text-blue-600 mt-1"
              />
              <div className="ml-3">
                <div className="flex items-center space-x-2">
                  <span className="font-medium text-gray-900">{option.label}</span>
                  {option.experimental && (
                    <span className="text-xs bg-amber-100 text-amber-700 px-2 py-0.5 rounded-full font-semibold">
                      Experimental
                    </span>
                  )}
                  <span className="text-xs bg-gray-200 text-gray-700 px-2 py-0.5 rounded">{option.size}</span>
                </div>
                <p className="text-sm text-gray-500 mt-1">{option.description}</p>
              </div>
            </label>
          ))}
        </div>
      </div>

      {/* Output Path */}
      <div className="bg-gray-50 rounded-lg p-6">
        <h3 className="text-lg font-semibold text-gray-900 mb-4">Output Location</h3>
        <div className="space-y-4">
          {isWdsPxeSelection ? (
            <div className="rounded-lg border border-blue-200 bg-blue-50 p-4">
              <label className="block text-sm font-medium text-blue-900 mb-1">Export Folder</label>
              <input
                type="text"
                value={wdsExportRoot}
                readOnly
                className="w-full rounded-lg border border-blue-200 bg-white px-4 py-2 text-gray-900"
              />
              <p className="mt-2 text-sm text-blue-900">
                BitOSDT exports `boot.wim`, the prepared Windows payload, a manifest, and WDS setup notes here. It does not copy the payload to your WDS, SMB, or HTTP location automatically.
              </p>
            </div>
          ) : (
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-1">Output Path</label>
              <div className="flex space-x-3">
                <input
                  type="text"
                  value={output.outputPath}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_OUTPUT', payload: { outputPath: e.target.value } })
                  }
                  placeholder="Output image path (.iso)"
                  className="flex-1 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                />
                <button
                  onClick={handleBrowse}
                  className="px-4 py-2 border border-gray-300 rounded-lg hover:bg-gray-50 text-gray-700"
                >
                  Browse
                </button>
              </div>
            </div>
          )}

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">Volume Label</label>
            <input
              type="text"
              value={output.volumeLabel}
              onChange={(e) =>
                dispatch({ type: 'UPDATE_OUTPUT', payload: { volumeLabel: e.target.value } })
              }
              placeholder="BITOSDT"
              maxLength={11}
              className="w-full md:w-64 px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
            />
            <p className="text-xs text-gray-500 mt-1">Maximum 11 characters</p>
          </div>
        </div>
      </div>

      {/* PXE / Lightweight Delivery */}
      {includesLightweightSelection && (
        <div className="bg-gray-50 rounded-lg p-6 space-y-6">
          <div>
            <h3 className="text-lg font-semibold text-gray-900 mb-2">PXE / Lightweight Delivery</h3>
            <p className="text-sm text-gray-500">
              Choose whether BitOSDT hosts the lightweight runtime for you or you provide the runtime URL and PXE export location manually.
            </p>
          </div>

          <div className="grid gap-4 md:grid-cols-2">
            {[
              {
                value: 'Simple',
                title: 'Simple',
                description: 'BitOSDT stages the lightweight files, hosts the runtime on port 8080, and shows the derived runtime URL.',
              },
              {
                value: 'Advanced',
                title: 'Advanced',
                description: 'Use your own runtime server URL and export the PXE/WDS files to a custom local or UNC path.',
              },
            ].map((option) => (
              <label
                key={option.value}
                className={`flex items-start rounded-lg border p-4 transition-colors ${output.deliveryMode === option.value
                  ? 'border-blue-500 bg-blue-50'
                  : 'border-gray-200 bg-white hover:border-blue-300'
                }`}
              >
                <input
                  type="radio"
                  name="deliveryMode"
                  checked={output.deliveryMode === option.value}
                  onChange={() =>
                    dispatch({
                      type: 'UPDATE_OUTPUT',
                      payload: { deliveryMode: option.value as 'Simple' | 'Advanced' },
                    })
                  }
                  className="mt-1 h-5 w-5 text-blue-600"
                />
                <div className="ml-3">
                  <p className="font-medium text-gray-900">{option.title}</p>
                  <p className="mt-1 text-sm text-gray-500">{option.description}</p>
                </div>
              </label>
            ))}
          </div>

          {output.deliveryMode === 'Simple' ? (
            <LightweightHostPanel
              description="Simple mode fixes the endpoint layout and uses the embedded BitOSDT lightweight host when you explicitly start it."
              helperText="A successful simple-mode build stages the PXE files here. The embedded host remains stopped until you click Start Host."
              refreshToken={hostPanelRefreshToken}
            />
          ) : (
            <div className="space-y-4 rounded-lg border border-gray-200 bg-white p-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Runtime Server URL
                </label>
                <input
                  type="text"
                  value={output.serverUrl || ''}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_OUTPUT', payload: { serverUrl: e.target.value } })
                  }
                  placeholder="http://deploy.local:8080"
                  className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
                />
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  PXE/WDS Export Path
                </label>
                <div className="flex gap-3">
                  <input
                    type="text"
                    value={output.pxeExportPath || ''}
                    onChange={(e) =>
                      dispatch({ type: 'UPDATE_OUTPUT', payload: { pxeExportPath: e.target.value } })
                    }
                    placeholder="\\\\wds-server\\reminst\\Boot\\BitOSDT"
                    className="flex-1 rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
                  />
                  <button
                    onClick={handleBrowsePxeExportPath}
                    className="rounded-lg border border-gray-300 px-4 py-2 text-gray-700 hover:bg-gray-50"
                  >
                    Browse
                  </button>
                </div>
                <p className="mt-1 text-xs text-gray-500">
                  Advanced mode exports the PXE/WDS boot files here and expects your runtime server URL to already host the BitOSDT endpoints.
                </p>
              </div>
            </div>
          )}

          <label className="flex items-center space-x-3">
            <input
              type="checkbox"
              checked={output.includeGui}
              onChange={(e) =>
                dispatch({ type: 'UPDATE_OUTPUT', payload: { includeGui: e.target.checked } })
              }
              className="w-5 h-5 text-blue-600 rounded"
            />
            <span className="text-gray-900">Include BitOSDT GUI in WinPE</span>
          </label>
        </div>
      )}

      {(includesFullIsoSelection || isWdsPxeSelection) && (
        <div className="bg-gray-50 rounded-lg p-6 space-y-4">
          <div>
            <h3 className="text-lg font-semibold text-gray-900 mb-2">
              {isWdsPxeSelection ? 'WDS/PXE Runtime Windows Image Path' : 'Full ISO PXE/WDS Fallback Image Sources'}
            </h3>
            <p className="text-sm text-gray-500">
              {isWdsPxeSelection
                ? 'Required: enter the final UNC or HTTP path where WinPE will reach the Windows payload during PXE deployment. BitOSDT will export the payload locally but will not publish it there for you.'
                : 'Optional: when WinPE cannot find a local install image, BitOSDT can fall back to UNC then HTTP sources.'}
            </p>
          </div>

          {isWdsPxeSelection ? (
            <div className="space-y-4">
              <div className="grid gap-4 md:grid-cols-2">
                {[
                  {
                    value: 'UNC',
                    title: 'UNC',
                    description: 'Use an SMB path that WinPE can access directly.',
                  },
                  {
                    value: 'HTTP',
                    title: 'HTTP',
                    description: 'Download the Windows payload from an HTTP or HTTPS URL.',
                  },
                ].map((option) => (
                  <label
                    key={option.value}
                    className={`flex items-start rounded-lg border p-4 transition-colors ${wdsRuntimeSource === option.value
                      ? 'border-blue-500 bg-blue-50'
                      : 'border-gray-200 bg-white hover:border-blue-300'
                    }`}
                  >
                    <input
                      type="radio"
                      name="wds-runtime-source"
                      checked={wdsRuntimeSource === option.value}
                      onChange={() => handleWdsRuntimeSourceChange(option.value as 'UNC' | 'HTTP')}
                      className="mt-1 h-5 w-5 text-blue-600"
                    />
                    <div className="ml-3">
                      <p className="font-medium text-gray-900">{option.title}</p>
                      <p className="mt-1 text-sm text-gray-500">{option.description}</p>
                    </div>
                  </label>
                ))}
              </div>

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  {wdsRuntimeSource === 'HTTP' ? 'Final HTTP Runtime URL' : 'Final UNC Runtime File Path'}
                </label>
                <input
                  type="text"
                  value={wdsRuntimeSource === 'HTTP' ? (output.fullIsoHttpUrl || '') : (output.fullIsoUncPath || '')}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_OUTPUT',
                      payload: wdsRuntimeSource === 'HTTP'
                        ? { fullIsoHttpUrl: e.target.value }
                        : { fullIsoUncPath: e.target.value },
                    })
                  }
                  placeholder={wdsRuntimeSource === 'HTTP' ? 'http://deploy.local/install.wim' : '\\\\wds-server\\reminst\\images\\install.wim'}
                  className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
                />
                {wdsRuntimeSource === 'UNC' && (
                  <p className="mt-2 text-sm text-gray-500">
                    Enter the full UNC file path that WinPE should open, for example
                    {' '}<code>\\\\wds-server\\reminst\\images\\install.wim</code>.
                  </p>
                )}
              </div>

              {shouldShowWdsUncAuth && (
                <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 space-y-4">
                  <p className="text-sm text-blue-900">
                    BitOSDT will authenticate to the UNC share in WinPE before opening the Windows image.
                  </p>
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      UNC Username
                    </label>
                    <input
                      type="text"
                      value={output.promptUncCredentialsAtRuntime ? '' : (output.fullIsoUncUsername || '')}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoUncUsername: e.target.value } })
                      }
                      placeholder="domain\\user or user@domain"
                      disabled={!!output.promptUncCredentialsAtRuntime}
                      className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 disabled:bg-gray-100 disabled:text-gray-400"
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      UNC Password
                    </label>
                    <input
                      type="password"
                      value={output.promptUncCredentialsAtRuntime ? '' : (output.fullIsoUncPassword || '')}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoUncPassword: e.target.value } })
                      }
                      placeholder={output.promptUncCredentialsAtRuntime ? 'Prompted at runtime' : 'Enter password'}
                      disabled={!!output.promptUncCredentialsAtRuntime}
                      className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 disabled:bg-gray-100 disabled:text-gray-400"
                    />
                  </div>

                  <label className="flex items-center gap-2 pt-1">
                    <input
                      type="checkbox"
                      checked={!!output.promptUncCredentialsAtRuntime}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { promptUncCredentialsAtRuntime: e.target.checked } })
                      }
                      className="w-4 h-4 text-blue-600 rounded border-gray-300"
                    />
                    <span className="text-sm text-gray-700">Prompt for credentials at runtime</span>
                  </label>
                  {output.promptUncCredentialsAtRuntime && (
                    <p className="text-xs text-gray-500">
                      Credentials will not be stored in the config file. You will be prompted when WinPE boots.
                    </p>
                  )}
                </div>
              )}
            </div>
          ) : (
            <>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  UNC Install Image File Path (optional)
                </label>
                <input
                  type="text"
                  value={output.fullIsoUncPath || ''}
                  onChange={(e) => dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoUncPath: e.target.value } })}
                  placeholder="\\\\wds-server\\reminst\\images\\install.wim"
                  className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
                />
                <p className="mt-2 text-sm text-gray-500">
                  Enter the full UNC file path that WinPE should use if it falls back to SMB, for example
                  {' '}<code>\\\\server\\share\\install.wim</code>.
                </p>
              </div>

              {shouldShowFullIsoUncAuth && (
                <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 space-y-4">
                  <p className="text-sm text-blue-900">
                    When WinPE falls back to this UNC image path, BitOSDT will authenticate before applying Windows.
                  </p>
                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      UNC Username
                    </label>
                    <input
                      type="text"
                      value={output.promptUncCredentialsAtRuntime ? '' : (output.fullIsoUncUsername || '')}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoUncUsername: e.target.value } })
                      }
                      placeholder="domain\\user or user@domain"
                      disabled={!!output.promptUncCredentialsAtRuntime}
                      className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 disabled:bg-gray-100 disabled:text-gray-400"
                    />
                  </div>

                  <div>
                    <label className="block text-sm font-medium text-gray-700 mb-1">
                      UNC Password
                    </label>
                    <input
                      type="password"
                      value={output.promptUncCredentialsAtRuntime ? '' : (output.fullIsoUncPassword || '')}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoUncPassword: e.target.value } })
                      }
                      placeholder={output.promptUncCredentialsAtRuntime ? 'Prompted at runtime' : 'Enter password'}
                      disabled={!!output.promptUncCredentialsAtRuntime}
                      className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900 disabled:bg-gray-100 disabled:text-gray-400"
                    />
                  </div>

                  <label className="flex items-center gap-2 pt-1">
                    <input
                      type="checkbox"
                      checked={!!output.promptUncCredentialsAtRuntime}
                      onChange={(e) =>
                        dispatch({ type: 'UPDATE_OUTPUT', payload: { promptUncCredentialsAtRuntime: e.target.checked } })
                      }
                      className="w-4 h-4 text-blue-600 rounded border-gray-300"
                    />
                    <span className="text-sm text-gray-700">Prompt for credentials at runtime</span>
                  </label>
                  {output.promptUncCredentialsAtRuntime && (
                    <p className="text-xs text-gray-500">
                      Credentials will not be stored in the config file. You will be prompted when WinPE boots.
                    </p>
                  )}
                </div>
              )}

              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  HTTP Install Image URL (optional)
                </label>
                <input
                  type="text"
                  value={output.fullIsoHttpUrl || ''}
                  onChange={(e) => dispatch({ type: 'UPDATE_OUTPUT', payload: { fullIsoHttpUrl: e.target.value } })}
                  placeholder="http://deploy.local/install.wim"
                  className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
                />
              </div>
            </>
          )}

          {isWdsPxeSelection && (
            <div className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900">
              Build flow:
              1. Import `boot.wim` from `C:\BitOSDT\WDS` into WDS.
              2. Copy `install.wim` to your final SMB or HTTP location.
              3. Make sure the hosted file path exactly matches the path entered above.
              4. PXE boot with WDS and let BitOSDT deploy from the remote image.
            </div>
          )}
        </div>
      )}

      {/* Boot Drivers */}
      <div className="bg-gray-50 rounded-lg p-6 space-y-4">
        <div className="flex items-start justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-gray-900">Boot Drivers</h3>
            <p className="text-sm text-gray-500">
              Add local driver folders for storage, RAID, virtIO, and NIC support in WinPE. Optional offline Windows injection applies only to Full ISO builds, plus direct WDS/PXE exports when enabled.
            </p>
          </div>
          <button
            onClick={handleAddDriverFolder}
            className="rounded-lg border border-gray-300 px-4 py-2 text-sm font-medium text-gray-700 hover:bg-white"
          >
            Add Driver Folder
          </button>
        </div>

        {output.driverPaths.length === 0 ? (
          <div className="rounded-lg border border-dashed border-gray-300 bg-white p-4 text-sm text-gray-500">
            No boot driver folders selected.
          </div>
        ) : (
          <div className="space-y-3">
            {output.driverPaths.map((item) => (
              <div key={item.sourcePath} className="flex items-start justify-between gap-3 rounded-lg border border-gray-200 bg-white p-4">
                <div>
                  <p className="font-medium text-gray-900">{deriveLocalPayloadDisplayName(item)}</p>
                  <p className="mt-1 break-all text-sm text-gray-500">{item.sourcePath}</p>
                </div>
                <button
                  onClick={() => removeDriverFolder(item.sourcePath)}
                  className="rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 hover:bg-gray-50"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        {isWdsPxeSelection && (
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">Boot Driver UNC Path (optional)</label>
            <input
              type="text"
              value={output.bootDriverUncPath || ''}
              onChange={(e) => dispatch({ type: 'UPDATE_OUTPUT', payload: { bootDriverUncPath: e.target.value } })}
              placeholder="\\\\wds-server\\drivers\\winpe"
              className="w-full rounded-lg border border-gray-300 px-4 py-2 text-gray-900"
            />
            <p className="mt-2 text-sm text-gray-500">
              Use this when the WDS/PXE build host should pull an additional boot-driver folder from SMB. Local folder selections remain supported and are combined with this UNC path during export.
            </p>
          </div>
        )}

        <label className="flex items-center space-x-3">
          <input
            type="checkbox"
            checked={output.applyDriversToOfflineWindows}
            onChange={(e) =>
              dispatch({ type: 'UPDATE_OUTPUT', payload: { applyDriversToOfflineWindows: e.target.checked } })
            }
            className="w-5 h-5 text-blue-600 rounded"
          />
          <span className="text-gray-900">Also inject selected boot drivers into the offline Windows image for Full ISO and WDS/PXE exports</span>
        </label>
        <p className="text-xs text-gray-500">
          In `Both` mode, offline Windows driver injection applies to the Full ISO half only. Lightweight output still uses boot drivers only for WinPE startup support.
        </p>
      </div>

      {/* Build Summary */}
      <div className="wizard-summary-card bg-blue-50 border-2 border-blue-300 rounded-lg p-6">
        <h3 className="text-lg font-bold text-blue-950 mb-3">Build Summary</h3>
        <div className="wizard-summary-grid grid grid-cols-2 gap-4 text-sm">
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Windows:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {windowsVersion.name} {windowsVersion.build} {windowsVersion.edition}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Source:</span>{' '}
            <span className={`wizard-wrap-anywhere font-medium ${hasValidSource ? 'text-green-700' : 'text-red-600'}`}>
              {windowsVersion.sourceType === 'cloud'
                ? (windowsVersion.downloadUrl ? 'Download from Microsoft CDN' : 'No download URL')
                : (windowsVersion.sourcePath
                  ? getBaseName(windowsVersion.sourcePath)
                  : 'No file selected')}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Output Type:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">{output.outputType}</span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Delivery:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {includesLightweightOutput ? output.deliveryMode : isWdsPxeSelection ? 'WDS/PXE export' : 'N/A'}
            </span>
          </div>
          {isWdsPxeSelection && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">Export Folder:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">{wdsExportRoot}</span>
            </div>
          )}
          {isWdsPxeSelection && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">Runtime Source:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">{wdsRuntimeSource}</span>
            </div>
          )}
          {isWdsPxeSelection && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">Runtime Path:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">
                {configuredWdsRuntimePath || 'Not configured'}
              </span>
            </div>
          )}
          {isWdsPxeSelection && output.bootDriverUncPath?.trim() && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">Boot Driver UNC:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">{output.bootDriverUncPath.trim()}</span>
            </div>
          )}
          {isWdsPxeSelection && wdsRuntimeSource === 'UNC' && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">UNC Auth:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">
                {hasConfiguredUncAuth ? 'Configured' : 'Not configured'}
              </span>
            </div>
          )}
          {!isWdsPxeSelection && output.fullIsoUncPath?.trim() && (
            <div className="wizard-summary-item">
              <span className="text-blue-800 font-semibold">UNC Fallback Auth:</span>{' '}
              <span className="wizard-wrap-anywhere font-medium text-gray-900">
                {hasConfiguredUncAuth ? 'Configured' : 'Not configured'}
              </span>
            </div>
          )}
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Users:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">{state.userAccounts.length} configured</span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Apps:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {state.apps.copiedItems.length + state.apps.wingetPackages.length + state.apps.chocolateyPackages.length + state.apps.customInstallers.length} selected
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Domain Join:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {state.domainJoin.enabled ? state.domainJoin.domain : 'Disabled'}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Windows Update:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">{state.windowsUpdate.enabled ? 'Enabled' : 'Disabled'}</span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Policies:</span>{' '}
            <span className={`wizard-wrap-anywhere font-medium ${readiness.unsupportedPolicySelectionsCount > 0 ? 'text-red-700' : 'text-gray-900'}`}>
              {totalPolicySelections > 0
                ? `${totalPolicySelections} selected${readiness.unsupportedPolicySelectionsCount > 0 ? ` (${readiness.unsupportedPolicySelectionsCount} blocking)` : ''}`
                : 'None'}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Computer Name:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {hasExplicitComputerName ? normalizedComputerName : 'Auto'}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Custom Scripts:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {state.apps.enableCustomScripts
                ? `${configuredCustomScriptsCount} enabled`
                : 'Disabled'}
            </span>
          </div>
          <div className="wizard-summary-item">
            <span className="text-blue-800 font-semibold">Boot Drivers:</span>{' '}
            <span className="wizard-wrap-anywhere font-medium text-gray-900">
              {hasBootDrivers || (isWdsPxeSelection && output.bootDriverUncPath?.trim())
                ? `${output.driverPaths.length} local folder${output.driverPaths.length === 1 ? '' : 's'}${isWdsPxeSelection && output.bootDriverUncPath?.trim() ? ' + UNC source' : ''}`
                : 'None'}
            </span>
          </div>
        </div>
        {!hasValidSource && (
          <div className="mt-4 p-3 bg-yellow-100 border border-yellow-300 rounded-lg">
            <p className="text-yellow-800 text-sm font-medium">
              {windowsVersion.sourceType === 'cloud'
                ? 'No download URL available. Please select a Windows version from the catalog in the Windows Source step.'
                : 'No Windows source file selected. Please go back to the Windows Source step and select an ISO, ESD, or WIM file.'}
            </p>
          </div>
        )}
      </div>

      {preflightError && (
        <div className="bg-red-50 border border-red-300 rounded-lg p-4">
          <p className="text-red-700 text-sm font-medium">{preflightError}</p>
        </div>
      )}

      {lightweightCompatibilityErrors.length > 0 && (
        <div className="bg-red-50 border border-red-300 rounded-lg p-4 space-y-1">
          {lightweightCompatibilityErrors.map((error) => (
            <p key={error} className="text-red-700 text-sm font-medium">
              {error}
            </p>
          ))}
        </div>
      )}

      {computerNameValidationError && (
        <div className="bg-red-50 border border-red-300 rounded-lg p-4">
          <p className="text-red-700 text-sm font-medium">{computerNameValidationError}</p>
        </div>
      )}

      {lightweightWarnings.length > 0 && (hasCustomInstallers || hasLocalPayloadCopyWork) && includesLightweightOutput && (
        <div className="bg-yellow-50 border border-yellow-300 rounded-lg p-4 space-y-1">
          {lightweightWarnings.map((warning) => (
            <p key={warning} className="text-yellow-800 text-sm font-medium">
              {warning}
            </p>
          ))}
        </div>
      )}

      {isWdsPxeSelection && wdsSignInWarnings.length > 0 && (
        <div className="bg-amber-50 border border-amber-300 rounded-lg p-4 space-y-1">
          {wdsSignInWarnings.map((warning) => (
            <p key={warning} className="text-amber-900 text-sm font-medium">
              {warning}
            </p>
          ))}
        </div>
      )}

      {/* Build Button */}
      <div className="flex justify-center space-x-4">
        <button
          onClick={() => setShowConfirm(true)}
          disabled={!canStartBuild}
          className={`px-8 py-3 text-lg font-semibold rounded-lg transition-colors ${!canStartBuild
              ? 'bg-gray-400 cursor-not-allowed text-white'
              : 'bg-green-600 hover:bg-green-700 text-white'
            }`}
        >
          {isCancelling ? 'Cancelling...' : isBuilding ? 'Building...' : 'Start Build'}
        </button>
        {isBuilding && (
          <button
            onClick={handleCancel}
            disabled={isCancelling}
            className="px-6 py-3 text-lg font-semibold rounded-lg bg-red-600 hover:bg-red-700 disabled:bg-red-300 disabled:cursor-not-allowed text-white transition-colors"
          >
            {isCancelling ? 'Cancelling...' : 'Cancel Build'}
          </button>
        )}
      </div>

      {workspaceRecoveryPrompt && (
        <AppModal
          open
          onClose={isRecoveringWorkspace ? undefined : () => setWorkspaceRecoveryPrompt(null)}
          labelledBy="workspace-recovery-title"
          closeOnBackdrop={!isRecoveringWorkspace}
          closeOnEscape={!isRecoveringWorkspace}
        >
          <>
            <div className="ops-modal-head">
              <div>
                <h3 id="workspace-recovery-title" className="ops-card-title">Recover Locked Workspace</h3>
                <p className="ops-card-subtitle">
                  BitOSDT found stale build artifacts that still appear to be owned by a previous DISM session.
                </p>
              </div>
            </div>
            <div className="ops-modal-body space-y-4">
              <div className="rounded-lg border border-amber-200 bg-amber-50 p-4 text-sm text-amber-900">
                {workspaceRecoveryPrompt.message}
              </div>
              {workspaceRecoveryPrompt.locked_path && (
                <div className="rounded-lg border border-gray-200 bg-gray-50 p-4 text-sm text-gray-700">
                  <p className="font-semibold text-gray-900">Locked Path</p>
                  <p className="break-all">{workspaceRecoveryPrompt.locked_path}</p>
                </div>
              )}
              <div className="space-y-3">
                <p className="text-sm font-semibold text-gray-900">Matched BitOSDT DISM processes</p>
                {workspaceRecoveryPrompt.processes.map((process) => (
                  <div key={process.pid} className="rounded-lg border border-gray-200 bg-gray-50 p-3 text-sm text-gray-700">
                    <p className="font-medium text-gray-900">
                      PID {process.pid} - {process.executable}
                    </p>
                    <p className="mt-1 break-all">{process.command_line}</p>
                  </div>
                ))}
              </div>
            </div>
            <div className="ops-modal-foot">
              <button
                type="button"
                onClick={() => setWorkspaceRecoveryPrompt(null)}
                disabled={isRecoveringWorkspace}
                className="ops-btn ops-btn-ghost"
              >
                Cancel Build
              </button>
              <button
                type="button"
                onClick={() => void handleWorkspaceRecovery()}
                disabled={isRecoveringWorkspace}
                className="ops-btn ops-btn-primary"
              >
                {isRecoveringWorkspace ? 'Stopping DISM...' : 'Stop Matched DISM And Continue'}
              </button>
            </div>
          </>
        </AppModal>
      )}

      {/* Build Confirmation Modal */}
      {showConfirm && (
        <AppModal open onClose={() => setShowConfirm(false)} size="compact" labelledBy="build-confirm-title">
          <>
            <div className="ops-modal-head">
              <div>
                <h3 id="build-confirm-title" className="ops-card-title">Confirm Build</h3>
                <p className="ops-card-subtitle">Review the final build settings before the backend pipeline starts.</p>
              </div>
            </div>
            <div className="ops-modal-body">
              <div className="bg-gray-50 rounded-lg p-4 space-y-2 text-sm">
              <div>
                <span className="font-medium text-gray-700">Source:</span>{' '}
                <span className="wizard-wrap-anywhere text-gray-900">
                  {windowsVersion.sourceType === 'cloud' ? 'Microsoft CDN' : windowsVersion.sourcePath ? getBaseName(windowsVersion.sourcePath) : 'Local file'}
                </span>
              </div>
              <div>
                <span className="font-medium text-gray-700">Output Type:</span>{' '}
                <span className="text-gray-900">{output.outputType}</span>
              </div>
              {includesLightweightSelection && (
                <div>
                  <span className="font-medium text-gray-700">Delivery:</span>{' '}
                  <span className="text-gray-900">{output.deliveryMode}</span>
                </div>
              )}
              <div>
                <span className="font-medium text-gray-700">
                  {isWdsPxeSelection ? 'Export Folder:' : 'Output Path:'}
                </span>{' '}
                <span className="wizard-wrap-anywhere text-gray-900">{isWdsPxeSelection ? wdsExportRoot : output.outputPath}</span>
              </div>
              {isWdsPxeSelection && (
                <div>
                  <span className="font-medium text-gray-700">Runtime Source:</span>{' '}
                  <span className="text-gray-900">{wdsRuntimeSource}</span>
                </div>
              )}
              {isWdsPxeSelection && (
                <div>
                  <span className="font-medium text-gray-700">Runtime Path:</span>{' '}
                  <span className="wizard-wrap-anywhere text-gray-900">
                    {configuredWdsRuntimePath || 'Not configured'}
                  </span>
                </div>
              )}
              {isWdsPxeSelection && output.bootDriverUncPath?.trim() && (
                <div>
                  <span className="font-medium text-gray-700">Boot Driver UNC:</span>{' '}
                  <span className="wizard-wrap-anywhere text-gray-900">{output.bootDriverUncPath.trim()}</span>
                </div>
              )}
              {isWdsPxeSelection && wdsRuntimeSource === 'UNC' && (
                <div>
                  <span className="font-medium text-gray-700">UNC Auth:</span>{' '}
                  <span className="text-gray-900">{hasConfiguredUncAuth ? 'Configured' : 'Not configured'}</span>
                </div>
              )}
              {!isWdsPxeSelection && output.fullIsoUncPath?.trim() && (
                <div>
                  <span className="font-medium text-gray-700">UNC Fallback Auth:</span>{' '}
                  <span className="text-gray-900">{hasConfiguredUncAuth ? 'Configured' : 'Not configured'}</span>
                </div>
              )}
              <div>
                <span className="font-medium text-gray-700">Boot Drivers:</span>{' '}
                <span className="text-gray-900">{buildDriverPaths.length}</span>
              </div>
              <div>
                <span className="font-medium text-gray-700">Policies:</span>{' '}
                <span className="text-gray-900">
                  {totalPolicySelections > 0
                    ? `${totalPolicySelections}${readiness.unsupportedPolicySelectionsCount > 0 ? ` (${readiness.unsupportedPolicySelectionsCount} blocking)` : ''}`
                    : 'None'}
                </span>
              </div>
              <div>
                <span className="font-medium text-gray-700">Computer Name:</span>{' '}
                <span className="text-gray-900">{hasExplicitComputerName ? normalizedComputerName : 'Auto'}</span>
              </div>
              <div>
                <span className="font-medium text-gray-700">Custom Scripts:</span>{' '}
                <span className="text-gray-900">
                  {state.apps.enableCustomScripts ? `${configuredCustomScriptsCount} enabled` : 'Disabled'}
                </span>
              </div>
            </div>
              {isEditing && (
                <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 space-y-2">
                <p className="text-sm font-semibold text-blue-900">Save Mode</p>
                <label className="flex items-start gap-2 text-sm text-blue-900">
                  <input
                    type="radio"
                    name="save-mode"
                    checked={saveMode === 'overwrite'}
                    onChange={() => setSaveMode('overwrite')}
                    className="mt-1"
                  />
                  <span>Overwrite existing image profile</span>
                </label>
                <label className="flex items-start gap-2 text-sm text-blue-900">
                  <input
                    type="radio"
                    name="save-mode"
                    checked={saveMode === 'copy'}
                    onChange={() => setSaveMode('copy')}
                    className="mt-1"
                  />
                  <span>Save as new copy</span>
                </label>
                </div>
              )}
            </div>
            <div className="ops-modal-foot">
              <button
                onClick={() => setShowConfirm(false)}
                className="ops-btn ops-btn-ghost"
              >
                Cancel
              </button>
              <button
                onClick={() => {
                  setShowConfirm(false);
                  void handleBuild();
                }}
                className="ops-btn ops-btn-primary"
              >
                Start Build
              </button>
            </div>
          </>
        </AppModal>
      )}

      {/* Progress Bar */}
      {isBuilding && (
        <div className="bg-gray-50 rounded-lg p-6">
          <div className="flex justify-between mb-2">
            <span className="text-sm font-medium text-gray-900">
              Current Step: <span className="text-blue-600">{formatBuildStep(currentStep || 'init')}</span>
            </span>
            <span className="text-sm font-medium text-gray-900">
              {buildProgressPercent}%
            </span>
          </div>
          <div className="w-full bg-gray-200 rounded-full h-2.5">
            <div
              className="bg-blue-600 h-2.5 rounded-full transition-all duration-500 ease-out"
              style={{ width: `${buildProgressPercent}%` }}
            ></div>
          </div>
        </div>
      )}

      {/* Build Progress Terminal */}
      {buildProgress.length > 0 && (
        <div className="bg-gray-950 rounded-lg p-4 font-mono text-sm border-2 border-gray-700 shadow-inner">
          <div className="flex items-center justify-between mb-2 pb-2 border-b border-gray-800">
            <span className="text-gray-400 font-semibold">Build Log</span>
            <div className="flex space-x-1.5">
              <div className="w-3 h-3 rounded-full bg-red-500"></div>
              <div className="w-3 h-3 rounded-full bg-yellow-500"></div>
              <div className="w-3 h-3 rounded-full bg-green-500"></div>
            </div>
          </div>
          <div ref={terminalRef} className="max-h-64 overflow-y-auto space-y-1">
            {buildProgress.map((line, i) => (
              <div key={i} className="py-0.5">
                {line.includes('Error') || line.includes('✗') ? (
                  <span className="text-red-400 font-semibold">{line}</span>
                ) : line.includes('complete') || line.includes('✓') ? (
                  <span className="text-green-400 font-semibold">{line}</span>
                ) : line.startsWith('[') ? (
                  <span className="text-cyan-400">{line}</span>
                ) : (
                  <span className="text-gray-300">{line}</span>
                )}
              </div>
            ))}
            {isBuilding && (
              <div className="text-green-400 animate-pulse">
                ▊ Processing...
              </div>
            )}
            {stallWarning && (
              <div className="text-yellow-400 font-bold mt-2 border border-yellow-500/30 bg-yellow-900/20 p-2 rounded">
                {stallWarning}
              </div>
            )}
          </div>
        </div>
      )}
      <CredentialWarningDialog
        open={showCredentialWarning}
        onDismiss={(suppressPermanently) => {
          setShowCredentialWarning(false);
          if (suppressPermanently) {
            setCredentialWarningSuppressed(true);
          }
          executeBuild();
        }}
      />
    </div>
  );
}
