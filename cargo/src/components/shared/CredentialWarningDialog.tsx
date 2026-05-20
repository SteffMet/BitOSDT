import { useId, useState } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { AppModal } from './AppModal';

interface CredentialWarningDialogProps {
  open: boolean;
  onDismiss: (suppressPermanently: boolean) => void;
}

export function CredentialWarningDialog({ open, onDismiss }: CredentialWarningDialogProps) {
  const [doNotShowAgain, setDoNotShowAgain] = useState(false);
  const titleId = useId();

  if (!open) return null;

  const handleDismiss = async () => {
    if (doNotShowAgain) {
      try {
        await invoke('set_credential_warning_suppressed', { suppressed: true });
      } catch {
        // If the Tauri call fails, continue anyway
      }
    }
    onDismiss(doNotShowAgain);
  };

  return (
    <AppModal open size="compact" onClose={handleDismiss} labelledBy={titleId}>
      <>
        <div className="ops-modal-body">
          <div className="flex items-start gap-3">
            <div className="flex-shrink-0 w-10 h-10 rounded-full bg-amber-100 flex items-center justify-center">
              <svg className="w-6 h-6 text-amber-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v2m0 4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
            </div>
            <div>
              <h3 id={titleId} className="text-lg font-semibold text-gray-900">Credentials Stored in Plain Text</h3>
              <p className="mt-2 text-sm text-gray-600">
                Credentials will be stored in plain text in the configuration file. Consider using the runtime prompt option for enhanced security.
              </p>
            </div>
          </div>

          <label className="flex items-start gap-3 cursor-pointer pt-2">
            <input
              type="checkbox"
              checked={doNotShowAgain}
              onChange={(e) => setDoNotShowAgain(e.target.checked)}
              className="w-4 h-4 mt-0.5 text-blue-600 rounded border-gray-300"
            />
            <span className="text-sm text-gray-700">Do not show this warning again</span>
          </label>
        </div>

        <div className="ops-modal-foot">
          <button
            type="button"
            className="ops-btn ops-btn-primary"
            onClick={handleDismiss}
          >
            I Understand
          </button>
        </div>
      </>
    </AppModal>
  );
}
