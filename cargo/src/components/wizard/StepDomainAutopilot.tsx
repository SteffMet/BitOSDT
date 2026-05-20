import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { useWizard } from './WizardContext';
import { CredentialWarningDialog } from '../shared/CredentialWarningDialog';

export function StepDomainAutopilot() {
  const { state, dispatch } = useWizard();
  const { domainJoin, autopilot } = state;
  const promptDomainCredentialsAtRuntime = !!domainJoin.promptForDomainCredentialsAtRuntime;
  const [showCredentialWarning, setShowCredentialWarning] = useState(false);
  const [credentialWarningSuppressed, setCredentialWarningSuppressed] = useState(false);

  useEffect(() => {
    invoke<boolean>('get_credential_warning_suppressed')
      .then((suppressed) => setCredentialWarningSuppressed(suppressed))
      .catch(() => setCredentialWarningSuppressed(false));
  }, []);

  return (
    <div className="wizard-step space-y-8">
      {/* Domain Join */}
      <div>
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-2xl font-bold text-gray-900">Domain Join</h2>
            <p className="text-gray-600">Join machines to an Active Directory domain.</p>
          </div>
          <label className="flex items-center space-x-2">
            <input
              type="checkbox"
              checked={domainJoin.enabled}
              onChange={(e) =>
                dispatch({ type: 'UPDATE_DOMAIN', payload: { enabled: e.target.checked } })
              }
              className="w-5 h-5 text-blue-600 rounded"
            />
            <span className="font-medium text-gray-900">Enable</span>
          </label>
        </div>

        {domainJoin.enabled && (
          <div className="bg-gray-50 rounded-lg p-6 space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  {promptDomainCredentialsAtRuntime ? 'Domain Name (Optional Pre-fill)' : 'Domain Name *'}
                </label>
                <input
                  type="text"
                  value={domainJoin.domain}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_DOMAIN', payload: { domain: e.target.value } })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  placeholder="contoso.com"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  OU Path (Optional)
                </label>
                <input
                  type="text"
                  value={domainJoin.ouPath || ''}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_DOMAIN', payload: { ouPath: e.target.value } })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  placeholder="OU=Computers,DC=contoso,DC=com"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Domain Admin Username *
                </label>
                <input
                  type="text"
                  value={promptDomainCredentialsAtRuntime ? '' : domainJoin.username}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_DOMAIN', payload: { username: e.target.value } })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900 disabled:bg-gray-100 disabled:text-gray-500"
                  placeholder={promptDomainCredentialsAtRuntime ? 'Prompted at runtime' : 'admin@contoso.com'}
                  disabled={promptDomainCredentialsAtRuntime}
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Domain Admin Password *
                </label>
                <input
                  type="password"
                  value={promptDomainCredentialsAtRuntime ? '' : domainJoin.password}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_DOMAIN', payload: { password: e.target.value } })
                  }
                  onBlur={() => {
                    if (!promptDomainCredentialsAtRuntime && domainJoin.password && !credentialWarningSuppressed) {
                      setShowCredentialWarning(true);
                    }
                  }}
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900 disabled:bg-gray-100 disabled:text-gray-500"
                  placeholder={promptDomainCredentialsAtRuntime ? 'Prompted at runtime' : '••••••••'}
                  disabled={promptDomainCredentialsAtRuntime}
                />
              </div>
            </div>
            <div className="space-y-3 pt-2 border-t border-gray-200">
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={promptDomainCredentialsAtRuntime}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_DOMAIN',
                      payload: { promptForDomainCredentialsAtRuntime: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <span className="text-gray-900">Prompt for domain credentials at runtime</span>
              </label>
              {promptDomainCredentialsAtRuntime && (
                <div className="bg-blue-50 border border-blue-200 rounded-lg p-3">
                  <p className="text-sm text-blue-800">
                    Domain credentials will not be stored. You will be prompted when WinPE boots.
                  </p>
                  <p className="text-xs text-blue-700 mt-1">
                    Leave Domain Name or OU Path blank if you want WinPE to prompt for those too.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Separator */}
      <div className="border-t border-gray-200"></div>

      {/* Autopilot */}
      <div>
        <div className="flex items-center justify-between mb-4">
          <div>
            <h2 className="text-2xl font-bold text-gray-900">Windows Autopilot</h2>
            <p className="text-gray-600">Configure Autopilot for cloud-based deployment.</p>
          </div>
          <label className="flex items-center space-x-2">
            <input
              type="checkbox"
              checked={autopilot.enabled}
              onChange={(e) =>
                dispatch({ type: 'UPDATE_AUTOPILOT', payload: { enabled: e.target.checked } })
              }
              className="w-5 h-5 text-blue-600 rounded"
            />
            <span className="font-medium text-gray-900">Enable</span>
          </label>
        </div>

        {autopilot.enabled && (
          <div className="bg-gray-50 rounded-lg p-6 space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Tenant ID *
                </label>
                <input
                  type="text"
                  value={autopilot.tenantId}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_AUTOPILOT', payload: { tenantId: e.target.value } })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Deployment Mode
                </label>
                <select
                  value={autopilot.deploymentMode}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_AUTOPILOT',
                      payload: {
                        deploymentMode: e.target.value as 'UserDriven' | 'SelfDeploying' | 'PreProvisioned',
                      },
                    })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                >
                  <option value="UserDriven">User-Driven</option>
                  <option value="SelfDeploying">Self-Deploying</option>
                  <option value="PreProvisioned">Pre-Provisioned (White Glove)</option>
                </select>
              </div>
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">
                  Group Tag (Optional)
                </label>
                <input
                  type="text"
                  value={autopilot.groupTag || ''}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_AUTOPILOT', payload: { groupTag: e.target.value } })
                  }
                  className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  placeholder="e.g., Sales, Engineering"
                />
              </div>
            </div>

            <div className="pt-4 border-t border-gray-200 space-y-3">
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={autopilot.skipUserOobe}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_AUTOPILOT', payload: { skipUserOobe: e.target.checked } })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <span className="text-gray-900">Skip User OOBE</span>
              </label>
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={autopilot.skipDeviceOobe}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_AUTOPILOT', payload: { skipDeviceOobe: e.target.checked } })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <span className="text-gray-900">Skip Device OOBE</span>
              </label>
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={autopilot.allowWhiteglove}
                  onChange={(e) =>
                    dispatch({ type: 'UPDATE_AUTOPILOT', payload: { allowWhiteglove: e.target.checked } })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <span className="text-gray-900">Allow White Glove (Pre-Provisioning)</span>
              </label>
            </div>
          </div>
        )}
      </div>

      {/* Info Box */}
      {!domainJoin.enabled && !autopilot.enabled && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800">
            <strong>Note:</strong> Neither domain join nor Autopilot is configured. 
            Machines will be set up as workgroup computers.
          </p>
        </div>
      )}
      <CredentialWarningDialog
        open={showCredentialWarning}
        onDismiss={(suppressPermanently) => {
          setShowCredentialWarning(false);
          if (suppressPermanently) {
            setCredentialWarningSuppressed(true);
          }
        }}
      />
    </div>
  );
}
