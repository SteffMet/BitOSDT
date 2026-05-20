import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { open } from '@tauri-apps/api/shell';
import { Copy, Download, FileUp, FolderOpen, HardDrive, Pencil, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { OpsPageShell } from '../layout/OpsPageShell';
import { useToast } from '../../contexts/ToastContext';
import { AppModal } from '../shared/AppModal';
import { UsbWriteModal, UsbWriteSelection } from '../usb/UsbWriteModal';
import type {
  OobeProfileDetail,
  PpkgCapabilityStatus,
  OobeProfilePreflight,
  OobeProfileRequest,
  OobeProfileSummary,
  PpkgRequest,
  PpkgResponse,
} from './oobeTypes';

interface ManageOobeProfilesProps {
  onBack: () => void;
  onCreateNew: () => void;
  onEditProfile: (name: string) => void;
}

function buildAutoPpkgPath(profilePath: string, profileName: string) {
  const cleaned = profilePath.replace(/[\\/]+$/, '');
  return `${cleaned}\\${profileName}.ppkg`;
}

function warningRequestsRegeneration(warning: string) {
  return /\bre-?generat(?:e|ed|ion)\b/i.test(warning);
}

export function ManageOobeProfiles({ onBack, onCreateNew, onEditProfile }: ManageOobeProfilesProps) {
  const { showToast } = useToast();
  const [profiles, setProfiles] = useState<OobeProfileSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [busyName, setBusyName] = useState<string | null>(null);
  const [advancedMode, setAdvancedMode] = useState(false);
  const [showCredentialPrompt, setShowCredentialPrompt] = useState(false);
  const [localAdminUsername, setLocalAdminUsername] = useState('');
  const [localAdminPassword, setLocalAdminPassword] = useState('');
  const [usbWriteProfile, setUsbWriteProfile] = useState<OobeProfileSummary | null>(null);
  const credentialResolverRef = useRef<((value: { username: string; password: string } | null) => void) | null>(null);

  const promptForLocalAdminCredentials = () =>
    new Promise<{ username: string; password: string } | null>((resolve) => {
      setLocalAdminUsername('');
      setLocalAdminPassword('');
      credentialResolverRef.current = resolve;
      setShowCredentialPrompt(true);
    });

  const getPpkgCapabilityStatus = async () => {
    try {
      return await invoke<PpkgCapabilityStatus>('get_ppkg_capability_status', { builderPath: null });
    } catch {
      const fallback: PpkgCapabilityStatus = {
        nativeBuilderAvailable: false,
        localAdminCredentialsRequired: true,
      };
      return fallback;
    }
  };

  const closeCredentialPrompt = (value: { username: string; password: string } | null) => {
    if (credentialResolverRef.current) {
      credentialResolverRef.current(value);
      credentialResolverRef.current = null;
    }
    setShowCredentialPrompt(false);
    setLocalAdminUsername('');
    setLocalAdminPassword('');
  };

  const loadProfiles = async () => {
    try {
      setLoading(true);
      const result = await invoke<OobeProfileSummary[]>('list_oobe_profiles');
      setProfiles(result);
    } catch (err) {
      console.error('Failed to load provisioning packages:', err);
      showToast('Failed to load provisioning packages', 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    setAdvancedMode(false);
    loadProfiles();
  }, []);

  const performAction = async (name: string, action: () => Promise<void>) => {
    try {
      setBusyName(name);
      await action();
      await loadProfiles();
    } finally {
      setBusyName(null);
    }
  };

  const handleDelete = async (name: string) => {
    if (!window.confirm(`Delete provisioning package '${name}'?`)) {
      return;
    }

    await performAction(name, async () => {
      await invoke('delete_oobe_profile', { name });
      showToast('Provisioning package deleted', 'success');
    });
  };

  const handleRename = async (name: string) => {
    const newName = window.prompt('Enter new profile name', name);
    if (!newName || newName.trim() === '' || newName.trim() === name) {
      return;
    }

    await performAction(name, async () => {
      await invoke('rename_oobe_profile', { name, newName: newName.trim() });
      showToast('Provisioning package renamed', 'success');
    });
  };

  const handleDuplicate = async (name: string) => {
    const newName = window.prompt('Enter duplicate profile name', `${name}-copy`);
    if (!newName || newName.trim() === '') {
      return;
    }

    await performAction(name, async () => {
      await invoke('duplicate_oobe_profile', { name, newName: newName.trim() });
      showToast('Provisioning package duplicated', 'success');
    });
  };

  const confirmPreflightWarnings = async (profileName: string, actionLabel: string) => {
    const preflight = await invoke<OobeProfilePreflight>('preflight_oobe_profile', { name: profileName });
    if (preflight.warnings.length === 0) {
      return true;
    }

    return window.confirm(
      `Preflight found ${preflight.warnings.length} warning(s) for '${profileName}'.\n\n${preflight.warnings
        .map((warning, index) => `${index + 1}. ${warning}`)
        .join('\n')}\n\nContinue ${actionLabel} anyway?`
    );
  };

  const handleExport = async (profile: OobeProfileSummary) => {
    const proceed = await confirmPreflightWarnings(profile.name, 'export');
    if (!proceed) {
      return;
    }

    const outputPath = await invoke<string | null>('show_save_dialog_with_filters', {
      defaultPath: `${profile.name}.zip`,
      title: 'Export Provisioning Package Profile',
      filters: [['ZIP Archive', ['zip']]],
    });

    if (!outputPath) {
      return;
    }

    await performAction(profile.name, async () => {
      await invoke('export_oobe_profile_zip', { name: profile.name, outputZipPath: outputPath });
      showToast('Provisioning profile exported from C:\\BitOSDT\\Provisioning', 'success');
    });
  };

  const handleExportPpkg = async (profile: OobeProfileSummary) => {
    const outputPath = await invoke<string | null>('show_save_dialog_with_filters', {
      defaultPath: `${profile.name}.ppkg`,
      title: 'Export Provisioning Package as PPKG',
      filters: [['Provisioning Package', ['ppkg']]],
    });

    if (!outputPath) {
      return;
    }

    const capability = await getPpkgCapabilityStatus();
    const credentials = capability.localAdminCredentialsRequired
      ? await promptForLocalAdminCredentials()
      : null;
    if (capability.localAdminCredentialsRequired && !credentials) {
      showToast('PPKG export canceled because local admin credentials were not provided for fallback mode.', 'warning');
      return;
    }

    await performAction(profile.name, async () => {
      const request: PpkgRequest = {
        profileName: profile.name,
        outputPpkgPath: outputPath,
        localAdminUsername: credentials?.username,
        localAdminPassword: credentials?.password,
      };
      const result = await invoke<PpkgResponse>('generate_oobe_ppkg', { request });
      if (result.warnings.length > 0) {
        showToast(`PPKG exported with warnings. Regenerated provisioning sidecar payload from the saved profile. Keep the .ppkg with sibling Scripts, Apps, and Files folders. Logs: ${result.logsPath}`, 'warning');
      } else {
        showToast(`Provisioning package exported: ${result.outputPpkgPath}. Provisioning sidecar payload regenerated from the saved profile. Keep the .ppkg with sibling Scripts, Apps, and Files folders.`, 'success');
      }
    });
  };

  const handleImport = async () => {
    try {
      const path = await invoke<string | null>('show_open_dialog', {
        title: 'Import Provisioning Package ZIP',
        filters: [['ZIP Archive', ['zip']], ['All Files', ['*']]],
      });
      if (!path) {
        return;
      }

      setBusyName('__import__');
      await invoke('import_oobe_profile_zip', { zipPath: path });
      showToast('Provisioning package imported', 'success');
      await loadProfiles();
    } catch (err) {
      console.error('Failed to import provisioning package:', err);
      showToast('Failed to import provisioning package', 'error');
    } finally {
      setBusyName(null);
    }
  };

  const handleRegenerate = async (name: string) => {
    const proceed = await confirmPreflightWarnings(name, 'regeneration');
    if (!proceed) {
      return;
    }

    const capability = await getPpkgCapabilityStatus();
    const credentials = capability.localAdminCredentialsRequired
      ? await promptForLocalAdminCredentials()
      : null;
    if (capability.localAdminCredentialsRequired && !credentials) {
      showToast('Regeneration canceled because local admin credentials were not provided for fallback mode.', 'warning');
      return;
    }

    await performAction(name, async () => {
      const detail = await invoke<OobeProfileDetail>('get_oobe_profile', { name });
      const request: OobeProfileRequest = { ...detail.request, overwrite: true };
      const summary = await invoke<OobeProfileSummary>('create_oobe_profile', { request });
      showToast('Provisioning profile regenerated into C:\\BitOSDT\\Provisioning', 'success');
      const autoPpkgPath = buildAutoPpkgPath(summary.path, summary.name);
      const ppkgRequest: PpkgRequest = {
        profileName: summary.name,
        outputPpkgPath: autoPpkgPath,
        localAdminUsername: credentials?.username,
        localAdminPassword: credentials?.password,
      };
      try {
        const result = await invoke<PpkgResponse>('generate_oobe_ppkg', { request: ppkgRequest });
        if (result.warnings.length > 0) {
          showToast(`PPKG generated with warnings. Provisioning sidecar payload regenerated from the saved profile. Keep the .ppkg with sibling Scripts, Apps, and Files folders. Logs: ${result.logsPath}`, 'warning');
        } else {
          showToast(`PPKG generated: ${result.outputPpkgPath}. Provisioning sidecar payload regenerated from the saved profile. Keep the .ppkg with sibling Scripts, Apps, and Files folders.`, 'success');
        }
      } catch (ppkgErr) {
        showToast(`Profile regenerated, but PPKG generation failed: ${String(ppkgErr)}`, 'warning');
      }
    });
  };

  const handleProvisioningUsbWrite = async (selection: UsbWriteSelection) => {
    if (!usbWriteProfile) {
      throw new Error('No provisioning profile selected.');
    }

    const capability = await getPpkgCapabilityStatus();
    const credentials = capability.localAdminCredentialsRequired
      ? await promptForLocalAdminCredentials()
      : null;
    if (capability.localAdminCredentialsRequired && !credentials) {
      throw new Error('USB write canceled because local admin credentials were not provided.');
    }

    const summary = await invoke<string>('write_provisioning_bundle_to_usb', {
      request: {
        profileName: usbWriteProfile.name,
        targetDiskNumber: selection.targetDiskNumber,
        confirmationToken: selection.confirmationToken,
        localAdminUsername: credentials?.username,
        localAdminPassword: credentials?.password,
      },
    });

    showToast(summary, 'success');
    setUsbWriteProfile(null);
  };

  if (loading) {
    return (
      <div className="ops-loading-screen">
        <div className="ops-spinner" />
        <p>Loading provisioning packages...</p>
      </div>
    );
  }

  return (
    <>
      <OpsPageShell
      kicker="Provisioning Package Library"
      title="Manage Provisioning Package"
      subtitle="Simple mode is default. Enable Advanced Mode to access import, export, and lifecycle tools."
      onBack={onBack}
      headerActions={
        <div className="ops-cluster">
          <button type="button" className="ops-btn ops-btn-secondary" onClick={onCreateNew}>
            <Plus size={15} />
            <span>Create Provisioning Package</span>
          </button>
          <button
            type="button"
            className={`ops-btn ${advancedMode ? 'ops-btn-primary' : 'ops-btn-ghost'}`}
            onClick={() => setAdvancedMode((prev) => !prev)}
          >
            <Pencil size={15} />
            <span>{advancedMode ? 'Advanced Mode: On' : 'Advanced Mode: Off'}</span>
          </button>
          {advancedMode ? (
            <button type="button" className="ops-btn ops-btn-secondary" onClick={handleImport} disabled={busyName === '__import__'}>
              <FileUp size={15} />
              <span>{busyName === '__import__' ? 'Importing...' : 'Import ZIP'}</span>
            </button>
          ) : null}
        </div>
      }
    >
      <div className="ops-layout-stack">
        {profiles.length === 0 ? (
          <section className="ops-card ops-empty-state">
            <h2 className="ops-card-title">No provisioning packages found</h2>
            <p className="ops-card-subtitle">Create your first provisioning package profile to populate this library.</p>
            <button type="button" className="ops-btn ops-btn-primary" onClick={onCreateNew}>
              <Plus size={15} />
              <span>Create Provisioning Package</span>
            </button>
          </section>
        ) : (
          <section className="ops-oobe-list">
            {profiles.map((profile) => {
              const isBusy = busyName === profile.name;
              const showRegenerateButton = advancedMode
                || (profile.preflightWarnings?.some((warning) => warningRequestsRegeneration(warning)) ?? false);
              return (
                <article key={profile.name} className="ops-card ops-oobe-list-item">
                  <div>
                    <h3 className="ops-card-title" style={{ marginBottom: '0.35rem' }}>{profile.name}</h3>
                    <p className="ops-card-subtitle">{profile.description || 'No description provided.'}</p>
                    <p className="ops-hint">{profile.path}</p>
                    <p className="ops-hint">Updated {new Date(profile.updatedAt).toLocaleString()}</p>
                    {(profile.preflightWarnings?.length ?? 0) > 0 ? (
                      <div className="ops-hint" style={{ color: '#b45309' }}>
                        <strong>Preflight warnings:</strong>
                        <ul style={{ margin: '0.35rem 0 0', paddingLeft: '1.1rem' }}>
                          {profile.preflightWarnings?.map((warning) => (
                            <li key={`${profile.name}-${warning}`}>{warning}</li>
                          ))}
                        </ul>
                      </div>
                    ) : null}
                  </div>

                  <div className="ops-cluster">
                    <button type="button" className="ops-btn ops-btn-secondary" onClick={() => open(profile.path)} disabled={isBusy}>
                      <FolderOpen size={15} />
                      <span>Open Folder</span>
                    </button>
                    <button type="button" className="ops-btn ops-btn-secondary" onClick={() => onEditProfile(profile.name)} disabled={isBusy}>
                      <Pencil size={15} />
                      <span>Edit</span>
                    </button>
                    <button
                      type="button"
                      className="ops-btn ops-btn-secondary"
                      onClick={() => setUsbWriteProfile(profile)}
                      disabled={isBusy}
                    >
                      <HardDrive size={15} />
                      <span>Write to USB</span>
                    </button>
                    <button type="button" className="ops-btn ops-btn-danger" onClick={() => handleDelete(profile.name)} disabled={isBusy}>
                      <Trash2 size={15} />
                      <span>Delete</span>
                    </button>

                    {advancedMode ? (
                      <>
                        <button type="button" className="ops-btn ops-btn-ghost" onClick={() => handleRename(profile.name)} disabled={isBusy}>
                          Rename
                        </button>
                        <button type="button" className="ops-btn ops-btn-ghost" onClick={() => handleDuplicate(profile.name)} disabled={isBusy}>
                          <Copy size={15} />
                          <span>Duplicate</span>
                        </button>
                        <button type="button" className="ops-btn ops-btn-ghost" onClick={() => handleExport(profile)} disabled={isBusy}>
                          <Download size={15} />
                          <span>Export ZIP</span>
                        </button>
                        <button type="button" className="ops-btn ops-btn-ghost" onClick={() => handleExportPpkg(profile)} disabled={isBusy}>
                          <Download size={15} />
                          <span>Export PPKG</span>
                        </button>
                      </>
                    ) : null}
                    {showRegenerateButton ? (
                      <button type="button" className="ops-btn ops-btn-ghost" onClick={() => handleRegenerate(profile.name)} disabled={isBusy}>
                        <RefreshCw size={15} />
                        <span>Regenerate</span>
                      </button>
                    ) : null}
                  </div>
                </article>
              );
            })}
          </section>
        )}
      </div>
      </OpsPageShell>
      {showCredentialPrompt ? (
        <AppModal open onClose={() => closeCredentialPrompt(null)} size="compact" labelledBy="manage-ppkg-local-admin-title">
          <>
            <div className="ops-modal-head">
              <div>
                <h2 id="manage-ppkg-local-admin-title">Local Admin Credential Required</h2>
                <p>Provide local admin credentials for environments that need ProvisioningTools fallback instead of native ICD.</p>
              </div>
            </div>
            <div className="ops-modal-body">
              <label className="ops-field">
                <span className="ops-label">Local admin username</span>
                <input
                  className="ops-input"
                  value={localAdminUsername}
                  onChange={(event) => setLocalAdminUsername(event.target.value)}
                  placeholder="Administrator"
                />
              </label>
              <label className="ops-field">
                <span className="ops-label">Local admin password</span>
                <input
                  type="password"
                  className="ops-input"
                  value={localAdminPassword}
                  onChange={(event) => setLocalAdminPassword(event.target.value)}
                  placeholder="Enter password"
                />
              </label>
            </div>
            <div className="ops-modal-foot">
              <button type="button" className="ops-btn ops-btn-ghost" onClick={() => closeCredentialPrompt(null)}>
                Cancel
              </button>
              <button
                type="button"
                className="ops-btn ops-btn-primary"
                onClick={() =>
                  closeCredentialPrompt({
                    username: localAdminUsername.trim(),
                    password: localAdminPassword,
                  })
                }
                disabled={!localAdminUsername.trim() || !localAdminPassword}
              >
                Continue
              </button>
            </div>
          </>
        </AppModal>
      ) : null}
      <UsbWriteModal
        isOpen={!!usbWriteProfile}
        title="Write Provisioning Bundle to USB"
        subtitle="BitOSDT will regenerate package payloads, wipe the selected removable disk, and stage autounattend plus PPKG sidecar layouts."
        confirmButtonLabel="Write Provisioning USB"
        onClose={() => setUsbWriteProfile(null)}
        onConfirm={handleProvisioningUsbWrite}
      />
    </>
  );
}

export default ManageOobeProfiles;
