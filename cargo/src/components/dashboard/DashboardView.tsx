import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { Activity, Boxes, Crosshair, FileCode2, FolderCog, HardDriveDownload, Monitor, Settings2 } from 'lucide-react';
import { invoke } from '@tauri-apps/api/tauri';
import { OpsPageShell } from '../layout/OpsPageShell';
import { useTheme } from '../../contexts/ThemeContext';
import { getThemeLabel } from '../../contexts/theme';
import type { OobeProfileSummary } from '../oobe/oobeTypes';

type DashboardImageProfile = {
  id: number | string;
};

type AppReleaseMetadata = {
  version: string;
  channel: 'release' | 'experimental';
};

interface DashboardViewProps {
  onStartWizard: () => void;
  onOpenSettings: () => void;
  onOpenImages: () => void;
  onCreateOobe: () => void;
  onManageOobe: () => void;
}

interface ActionTileProps {
  icon: ReactNode;
  title: string;
  subtitle: string;
  onClick: () => void;
  badge?: string;
}

function ActionTile({ icon, title, subtitle, onClick, badge }: ActionTileProps) {
  return (
    <button type="button" onClick={onClick} className="ops-action-tile">
      <span className="ops-action-tile-border" aria-hidden="true" />
      <span className="ops-action-inner">
        <span className="ops-action-icon">{icon}</span>
        <span className="ops-action-title">
          {title}
          {badge ? <span className="ops-exp-badge">{badge}</span> : null}
        </span>
        <span className="ops-action-subtitle">{subtitle}</span>
      </span>
    </button>
  );
}

export function DashboardView({
  onStartWizard,
  onOpenSettings,
  onOpenImages,
  onCreateOobe,
  onManageOobe,
}: DashboardViewProps) {
  const [version, setVersion] = useState('2.0.6');
  const [releaseChannel, setReleaseChannel] = useState<'release' | 'experimental'>('release');
  const [imageCount, setImageCount] = useState(0);
  const [provisioningPackageCount, setProvisioningPackageCount] = useState(0);
  const { theme, effectiveTheme } = useTheme();

  useEffect(() => {
    invoke<string>('get_app_version').then(setVersion).catch(() => {});
    invoke<AppReleaseMetadata>('get_app_release_metadata')
      .then((metadata) => setReleaseChannel(metadata.channel))
      .catch(() => {});
    invoke<DashboardImageProfile[]>('list_images')
      .then((images) => setImageCount(images.length))
      .catch(() => setImageCount(0));
    invoke<OobeProfileSummary[]>('list_oobe_profiles')
      .then((profiles) => setProvisioningPackageCount(profiles.length))
      .catch(() => setProvisioningPackageCount(0));
  }, []);

  return (
    <OpsPageShell
      kicker="BitOSDT Mission Control"
      title="Deployment Command Console"
      subtitle="Create, review, and tune deployment images from one focused workspace."
      headerActions={<span className="ops-meta-pill">{`Version ${version} (${releaseChannel})`}</span>}
    >
      <div className="ops-layout-stack">
        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon">
              <Crosshair size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">Quick Actions</h2>
              <p className="ops-card-subtitle">Jump straight to the workflow you need.</p>
            </div>
          </div>
          <div className="ops-actions-grid">
            <ActionTile
              icon={<HardDriveDownload size={18} />}
              title="Create New Image"
              subtitle="Launch the guided deployment build wizard."
              onClick={onStartWizard}
            />
            <ActionTile
              icon={<Boxes size={18} />}
              title="Manage Images"
              subtitle="Inspect, duplicate, and remove existing image profiles."
              onClick={onOpenImages}
            />
            <ActionTile
              icon={<FileCode2 size={18} />}
              title="Create Provisioning Package"
              subtitle="Build a new provisioning package profile."
              onClick={onCreateOobe}
              badge="Experimental"
            />
            <ActionTile
              icon={<FolderCog size={18} />}
              title="Manage Provisioning Package"
              subtitle="Review and maintain provisioning package profiles."
              onClick={onManageOobe}
              badge="Experimental"
            />
            <ActionTile
              icon={<Settings2 size={18} />}
              title="Settings"
              subtitle="Configure paths, catalogs, and sync defaults."
              onClick={onOpenSettings}
            />
          </div>
        </section>

        <section className="ops-card">
          <div className="ops-card-heading">
            <span className="ops-card-icon ops-card-icon-signal">
              <Activity size={16} />
            </span>
            <div>
              <h2 className="ops-card-title">Operations Snapshot</h2>
              <p className="ops-card-subtitle">Current workspace state and recommended next steps.</p>
            </div>
          </div>
          <div className="ops-stats-grid">
            <article className="ops-stat-card ops-stat-card-cyan">
              <p className="ops-stat-label">Image Profiles</p>
              <p className="ops-stat-value">{imageCount}</p>
            </article>
            <article className="ops-stat-card ops-stat-card-orange">
              <p className="ops-stat-label">Provisioning Packages</p>
              <p className="ops-stat-value">{provisioningPackageCount}</p>
            </article>
            <article className="ops-stat-card ops-stat-card-blue">
              <p className="ops-stat-label">Default Theme</p>
              <p className="ops-stat-value">
                {getThemeLabel(theme, effectiveTheme)}
              </p>
            </article>
          </div>
          <div className="ops-info-row">
            <span className="ops-info-icon">
              <Monitor size={14} />
            </span>
            <p className="ops-info-text">
              Tip: run the wizard after verifying catalogs and paths in Settings for consistent build output.
            </p>
          </div>
        </section>
      </div>
    </OpsPageShell>
  );
}
