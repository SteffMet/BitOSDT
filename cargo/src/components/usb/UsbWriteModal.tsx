import { useEffect, useId, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { AppModal } from '../shared/AppModal';

export interface UsbTarget {
  diskNumber: number;
  friendlyName: string;
  sizeBytes: number;
  busType: string;
  driveLetters: string[];
  isSystem: boolean;
  isBoot: boolean;
  isReadOnly: boolean;
  confirmationPhrase: string;
}

export interface UsbWriteSelection {
  targetDiskNumber: number;
  confirmationToken: string;
}

interface UsbWriteModalProps {
  isOpen: boolean;
  title: string;
  subtitle: string;
  confirmButtonLabel: string;
  onClose: () => void;
  onConfirm: (selection: UsbWriteSelection) => Promise<void>;
}

function formatBytes(bytes: number) {
  if (!bytes || bytes <= 0) {
    return 'Unknown size';
  }
  const gb = bytes / (1024 * 1024 * 1024);
  return `${gb.toFixed(2)} GB`;
}

export function UsbWriteModal({
  isOpen,
  title,
  subtitle,
  confirmButtonLabel,
  onClose,
  onConfirm,
}: UsbWriteModalProps) {
  const titleId = useId();
  const [targets, setTargets] = useState<UsbTarget[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedDiskNumber, setSelectedDiskNumber] = useState<number | null>(null);
  const [confirmationInput, setConfirmationInput] = useState('');

  useEffect(() => {
    let cancelled = false;
    const loadTargets = async () => {
      if (!isOpen) {
        return;
      }
      setLoading(true);
      setError(null);
      setConfirmationInput('');
      setSelectedDiskNumber(null);
      try {
        const rows = await invoke<UsbTarget[]>('list_usb_targets');
        if (!cancelled) {
          setTargets(rows);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(String(loadError));
          setTargets([]);
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };

    void loadTargets();
    return () => {
      cancelled = true;
    };
  }, [isOpen]);

  const selectedTarget = useMemo(
    () => targets.find((target) => target.diskNumber === selectedDiskNumber) ?? null,
    [targets, selectedDiskNumber],
  );

  const canConfirm =
    !!selectedTarget &&
    confirmationInput.trim().toLowerCase() === selectedTarget.confirmationPhrase.toLowerCase() &&
    !busy;

  const handleConfirm = async () => {
    if (!selectedTarget || !canConfirm) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await onConfirm({
        targetDiskNumber: selectedTarget.diskNumber,
        confirmationToken: confirmationInput.trim(),
      });
      onClose();
    } catch (confirmError) {
      setError(String(confirmError));
    } finally {
      setBusy(false);
    }
  };

  if (!isOpen) {
    return null;
  }

  return (
    <AppModal
      open
      onClose={busy ? undefined : onClose}
      labelledBy={titleId}
      closeOnBackdrop={!busy}
      closeOnEscape={!busy}
    >
      <>
        <div className="ops-modal-head">
          <div>
            <h2 id={titleId}>{title}</h2>
            <p>{subtitle}</p>
          </div>
        </div>
        <div className="ops-modal-body space-y-4">
          {loading ? (
            <p>Scanning removable drives...</p>
          ) : (
            <div className="space-y-3">
              {targets.length === 0 ? (
                <p className="ops-hint">No removable USB targets detected.</p>
              ) : (
                targets.map((target) => (
                  <label
                    key={target.diskNumber}
                    className={`flex items-start gap-3 rounded-lg border p-3 ${
                      selectedDiskNumber === target.diskNumber
                        ? 'border-blue-500 bg-blue-50'
                        : 'border-gray-200 bg-white'
                    }`}
                  >
                    <input
                      type="radio"
                      name="usb-target"
                      checked={selectedDiskNumber === target.diskNumber}
                      onChange={() => setSelectedDiskNumber(target.diskNumber)}
                      className="mt-1"
                    />
                    <div>
                      <p className="font-semibold text-gray-900">
                        Disk {target.diskNumber} - {target.friendlyName || 'Removable Drive'}
                      </p>
                      <p className="text-sm text-gray-600">
                        {formatBytes(target.sizeBytes)} | {target.busType || 'Unknown bus'} | Letters:{' '}
                        {target.driveLetters.length > 0 ? target.driveLetters.join(', ') : 'none'}
                      </p>
                    </div>
                  </label>
                ))
              )}
            </div>
          )}

          {selectedTarget && (
            <label className="ops-field">
              <span className="ops-label">
                Type <code>{selectedTarget.confirmationPhrase}</code> to confirm destructive wipe
              </span>
              <input
                className="ops-input"
                value={confirmationInput}
                onChange={(event) => setConfirmationInput(event.target.value)}
                placeholder={selectedTarget.confirmationPhrase}
              />
            </label>
          )}

          {error && (
            <div className="rounded-lg border border-red-300 bg-red-50 p-3 text-sm text-red-700">
              {error}
            </div>
          )}

          {busy && (
            <div className="rounded-lg border border-blue-300 bg-blue-50 p-3 text-sm text-blue-800">
              Writing to USB is in progress. Keep this window open until completion.
            </div>
          )}
        </div>
        <div className="ops-modal-foot">
          <button type="button" className="ops-btn ops-btn-ghost" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="ops-btn ops-btn-danger"
            onClick={() => void handleConfirm()}
            disabled={!canConfirm}
          >
            {busy ? 'Writing...' : confirmButtonLabel}
          </button>
        </div>
      </>
    </AppModal>
  );
}
