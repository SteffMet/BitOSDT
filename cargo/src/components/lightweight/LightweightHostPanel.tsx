import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { FolderTree, Play, RefreshCw, ServerCog, Square } from 'lucide-react';
import type { LightweightHostStatus, SimpleDeliveryDefaults } from './lightweightHostTypes';

interface LightweightHostPanelProps {
  title?: string;
  description: string;
  helperText: string;
  refreshToken?: number;
}

type HostAction = 'refresh' | 'start' | 'stop' | null;

export function LightweightHostPanel({
  title = 'Lightweight PXE Host',
  description,
  helperText,
  refreshToken = 0,
}: LightweightHostPanelProps) {
  const [defaults, setDefaults] = useState<SimpleDeliveryDefaults | null>(null);
  const [status, setStatus] = useState<LightweightHostStatus | null>(null);
  const [busyAction, setBusyAction] = useState<HostAction>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const loadDefaults = async () => {
    try {
      const nextDefaults = await invoke<SimpleDeliveryDefaults>('get_simple_delivery_defaults');
      setDefaults(nextDefaults);
      setActionError(null);
    } catch (error) {
      setActionError(String(error));
    }
  };

  const refreshStatus = async () => {
    try {
      const nextStatus = await invoke<LightweightHostStatus>('get_lightweight_host_status');
      setStatus(nextStatus);
      setActionError(null);
    } catch (error) {
      setActionError(String(error));
    }
  };

  useEffect(() => {
    void loadDefaults();
    void refreshStatus();
  }, [refreshToken]);

  const runAction = async (action: Exclude<HostAction, null>) => {
    setBusyAction(action);
    try {
      if (action === 'refresh') {
        await refreshStatus();
      } else if (action === 'start') {
        const nextStatus = await invoke<LightweightHostStatus>('start_lightweight_host');
        setStatus(nextStatus);
        setActionError(null);
      } else {
        await invoke('stop_lightweight_host');
        setActionError(null);
        await refreshStatus();
      }
    } catch (error) {
      setActionError(String(error));
    } finally {
      setBusyAction(null);
    }
  };

  const hostRunning = status?.running ?? false;

  return (
    <section className="ops-card ops-lightweight-panel">
      <div className="ops-card-heading">
        <span className="ops-card-icon">
          <ServerCog size={16} />
        </span>
        <div>
          <h3 className="ops-card-title">{title}</h3>
          <p className="ops-card-subtitle">{description}</p>
        </div>
      </div>

      <div className="ops-detail-grid">
        <div>
          <span>Runtime URL</span>
          <strong className="ops-break">{defaults?.runtimeUrl || 'Resolving...'}</strong>
        </div>
        <div>
          <span>PXE Staging Path</span>
          <strong className="ops-break">{defaults?.publishPath || 'Resolving...'}</strong>
        </div>
      </div>

      <p className="ops-hint">{helperText}</p>

      <div className="ops-cluster">
        <span className={hostRunning ? 'ops-pill ops-pill-ready' : 'ops-pill ops-pill-draft'}>
          {hostRunning ? 'Host running' : 'Host stopped'}
        </span>
        <button
          type="button"
          onClick={() => void runAction('refresh')}
          disabled={busyAction !== null}
          className="ops-btn ops-btn-secondary"
        >
          <RefreshCw size={15} />
          <span>{busyAction === 'refresh' ? 'Refreshing...' : 'Refresh Status'}</span>
        </button>
        <button
          type="button"
          onClick={() => void runAction('start')}
          disabled={busyAction !== null || hostRunning}
          className="ops-btn ops-btn-primary"
        >
          <Play size={15} />
          <span>{busyAction === 'start' ? 'Starting...' : 'Start Host'}</span>
        </button>
        <button
          type="button"
          onClick={() => void runAction('stop')}
          disabled={busyAction !== null || !hostRunning}
          className="ops-btn ops-btn-ghost"
        >
          <Square size={15} />
          <span>{busyAction === 'stop' ? 'Stopping...' : 'Stop Host'}</span>
        </button>
      </div>

      <div className="ops-detail-grid">
        <div>
          <span>Bind Address</span>
          <strong className="ops-break">{status?.bindAddress || defaults?.bindAddress || 'Resolving...'}</strong>
        </div>
        <div>
          <span>Serving Path</span>
          <strong className="ops-break">{status?.stagingPath || defaults?.publishPath || 'Resolving...'}</strong>
        </div>
      </div>

      {(actionError || status?.lastError) && (
        <div className="ops-lightweight-alert ops-lightweight-alert-danger">
          <FolderTree size={15} />
          <p>{actionError || status?.lastError}</p>
        </div>
      )}
    </section>
  );
}
