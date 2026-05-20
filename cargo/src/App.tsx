import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { open } from '@tauri-apps/api/shell';
import { ImageWizard } from './components/wizard';
import { Settings } from './components/Settings';
import { ImageManager } from './components/ImageManager';
import { DashboardView } from './components/dashboard/DashboardView';
import { CreateOobeProfile } from './components/oobe/CreateOobeProfile';
import { ManageOobeProfiles } from './components/oobe/ManageOobeProfiles';
import { AppTitleBar } from './components/AppTitleBar';
import { AppModal } from './components/shared/AppModal';
import './App.css';

type View = 'dashboard' | 'wizard' | 'settings' | 'images' | 'create-oobe' | 'manage-oobe';

type ReleaseChannel = 'release' | 'experimental';

interface UpdateCheckResponse {
  currentVersion: string;
  currentChannel: ReleaseChannel;
  latestVersion: string | null;
  latestChannel: ReleaseChannel | null;
  forumUrl: string;
  title: string | null;
  publishedAt: string | null;
  updateAvailable: boolean;
}

interface ImageEditPayload {
  wizardState: unknown;
  legacyDefaultsApplied: boolean;
  legacyWarning?: string | null;
}

function UpdateAvailableModal({
  update,
  onDismiss,
}: {
  update: UpdateCheckResponse;
  onDismiss: () => void;
}) {
  const channelLabel = update.latestChannel === 'experimental' ? 'Experimental' : 'Release';

  return (
    <AppModal open onClose={onDismiss} size="compact" labelledBy="update-available-title">
      <>
        <div className="ops-modal-head">
          <div>
            <h2 id="update-available-title">Update Available</h2>
            <p>A newer downloadable BitOSDT build is available for this channel.</p>
          </div>
        </div>

        <div className="ops-modal-body">
          <div className="ops-layout-stack ops-compact-stack">
            <p>
              <strong>Installed:</strong> {update.currentVersion} ({update.currentChannel})
            </p>
            <p>
              <strong>Latest:</strong> {update.latestVersion ?? 'Unknown'} ({channelLabel})
            </p>
            {update.title ? <p>{update.title}</p> : null}
          </div>
        </div>

        <div className="ops-modal-foot">
          <button type="button" onClick={onDismiss} className="ops-btn ops-btn-ghost">
            Dismiss
          </button>
          <button
            type="button"
            className="ops-btn ops-btn-primary"
            onClick={() => open(update.forumUrl || 'https://bitosdt.com/forum/')}
          >
            Download
          </button>
        </div>
      </>
    </AppModal>
  );
}

function App() {
  const [view, setView] = useState<View>('dashboard');
  const [editingOobeProfile, setEditingOobeProfile] = useState<string | null>(null);
  const [editingImageId, setEditingImageId] = useState<string | null>(null);
  const [editingImagePayload, setEditingImagePayload] = useState<ImageEditPayload | null>(null);
  const [updateNotice, setUpdateNotice] = useState<UpdateCheckResponse | null>(null);
  const activeEditImageIdRef = useRef<string | null>(null);

  useEffect(() => {
    let active = true;

    invoke<UpdateCheckResponse>('check_for_app_update')
      .then((result) => {
        if (!active || !result.updateAvailable) {
          return;
        }
        setUpdateNotice(result);
      })
      .catch((error) => {
        console.warn('Update check failed:', error);
      });

    return () => {
      active = false;
    };
  }, []);

  const content = (() => {
    if (view === 'wizard') {
      return (
        <ImageWizard
          editingImageId={editingImageId}
          initialEditPayload={editingImagePayload}
          onExit={() => {
            activeEditImageIdRef.current = null;
            setEditingImageId(null);
            setEditingImagePayload(null);
            setView('dashboard');
          }}
        />
      );
    }
    if (view === 'settings') {
      return <Settings onBack={() => setView('dashboard')} />;
    }
    if (view === 'images') {
      return (
        <ImageManager
          onBack={() => setView('dashboard')}
          onStartWizard={(imageId?: string | null) => {
            const normalizedImageId = imageId ?? null;
            activeEditImageIdRef.current = normalizedImageId;
            setEditingImageId(normalizedImageId);
            setEditingImagePayload(null);
            setView('wizard');
            if (imageId) {
              void invoke<ImageEditPayload>('get_image_edit_payload', { imageId })
                .then((payload) => {
                  if (activeEditImageIdRef.current === imageId) {
                    setEditingImagePayload(payload);
                  }
                })
                .catch((error) => {
                  if (activeEditImageIdRef.current === imageId) {
                    console.error('Failed to preload image edit payload:', error);
                    setEditingImagePayload(null);
                  }
                });
            }
          }}
        />
      );
    }
    if (view === 'create-oobe') {
      return (
        <CreateOobeProfile
          onBack={() => {
            if (editingOobeProfile) {
              setView('manage-oobe');
              setEditingOobeProfile(null);
              return;
            }
            setView('dashboard');
          }}
          onOpenManage={() => {
            setEditingOobeProfile(null);
            setView('manage-oobe');
          }}
          editingProfileName={editingOobeProfile}
          onClearEditing={() => setEditingOobeProfile(null)}
        />
      );
    }
    if (view === 'manage-oobe') {
      return (
        <ManageOobeProfiles
          onBack={() => setView('dashboard')}
          onCreateNew={() => {
            setEditingOobeProfile(null);
            setView('create-oobe');
          }}
          onEditProfile={(name) => {
            setEditingOobeProfile(name);
            setView('create-oobe');
          }}
        />
      );
    }

    return (
      <DashboardView
        onStartWizard={() => {
          activeEditImageIdRef.current = null;
          setEditingImageId(null);
          setEditingImagePayload(null);
          setView('wizard');
        }}
        onOpenSettings={() => setView('settings')}
        onOpenImages={() => setView('images')}
        onCreateOobe={() => {
          setEditingOobeProfile(null);
          setView('create-oobe');
        }}
        onManageOobe={() => {
          setEditingOobeProfile(null);
          setView('manage-oobe');
        }}
      />
    );
  })();

  return (
    <div className="app-shell">
      <div className="space-stars-bg" aria-hidden="true" />
      <div className="space-dust-bg" aria-hidden="true" />
      <div className="trajectory-line" aria-hidden="true" />
      <AppTitleBar />
      <div className="app-content">{content}</div>
      {updateNotice ? <UpdateAvailableModal update={updateNotice} onDismiss={() => setUpdateNotice(null)} /> : null}
    </div>
  );
}

export default App;
