import { useWizard } from './WizardContext';

export function StepWindowsUpdate() {
  const { state, dispatch } = useWizard();
  const { windowsUpdate } = state;

  return (
    <div className="wizard-step space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-gray-900 mb-2">Windows Update</h2>
          <p className="text-gray-600">
            Configure which updates to install during the image creation process.
          </p>
        </div>
        <label className="flex items-center space-x-2">
          <input
            type="checkbox"
            checked={windowsUpdate.enabled}
            onChange={(e) =>
              dispatch({ type: 'UPDATE_WINDOWS_UPDATE', payload: { enabled: e.target.checked } })
            }
            className="w-5 h-5 text-blue-600 rounded"
          />
          <span className="font-medium text-gray-900">Enable Updates</span>
        </label>
      </div>

      {windowsUpdate.enabled && (
        <>
          {/* Update Categories */}
          <div className="bg-gray-50 rounded-lg p-6">
            <h3 className="text-lg font-semibold mb-4">Update Categories</h3>
            <div className="space-y-3">
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={windowsUpdate.installSecurityUpdates}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_WINDOWS_UPDATE',
                      payload: { installSecurityUpdates: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <div>
                  <span className="font-medium text-gray-900">Security Updates</span>
                  <p className="text-sm text-gray-500">Critical security patches and fixes</p>
                </div>
              </label>

              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={windowsUpdate.installCriticalUpdates}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_WINDOWS_UPDATE',
                      payload: { installCriticalUpdates: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <div>
                  <span className="font-medium text-gray-900">Critical Updates</span>
                  <p className="text-sm text-gray-500">Important non-security fixes</p>
                </div>
              </label>

              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={windowsUpdate.installDriverUpdates}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_WINDOWS_UPDATE',
                      payload: { installDriverUpdates: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <div>
                  <span className="font-medium text-gray-900">Driver Updates</span>
                  <p className="text-sm text-gray-500">Updated drivers from Windows Update</p>
                </div>
              </label>
            </div>
          </div>

          {/* Exclusions */}
          <div className="bg-gray-50 rounded-lg p-6">
            <h3 className="text-lg font-semibold mb-4">Exclusions</h3>
            <div className="space-y-3">
              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={windowsUpdate.excludePreview}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_WINDOWS_UPDATE',
                      payload: { excludePreview: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <div>
                  <span className="font-medium text-gray-900">Exclude Preview Updates</span>
                  <p className="text-sm text-gray-500">Skip preview/beta updates</p>
                </div>
              </label>

              <label className="flex items-center space-x-3">
                <input
                  type="checkbox"
                  checked={windowsUpdate.excludeOptional}
                  onChange={(e) =>
                    dispatch({
                      type: 'UPDATE_WINDOWS_UPDATE',
                      payload: { excludeOptional: e.target.checked },
                    })
                  }
                  className="w-5 h-5 text-blue-600 rounded"
                />
                <div>
                  <span className="font-medium text-gray-900">Exclude Optional Updates</span>
                  <p className="text-sm text-gray-500">Skip optional feature updates</p>
                </div>
              </label>
            </div>
          </div>

          {/* Reboot Behavior */}
          <div className="bg-gray-50 rounded-lg p-6">
            <h3 className="text-lg font-semibold mb-4">Reboot Behavior</h3>
            <div className="space-y-3">
              {[
                {
                  value: 'NoReboot',
                  label: 'No Automatic Reboot',
                  description: 'Updates installed but no reboot during build',
                },
                {
                  value: 'AutoReboot',
                  label: 'Automatic Reboot',
                  description: 'Reboot automatically after updates',
                },
                {
                  value: 'ScheduleReboot',
                  label: 'Schedule Reboot',
                  description: 'Schedule reboot for later',
                },
              ].map((option) => (
                <label key={option.value} className="flex items-center space-x-3">
                  <input
                    type="radio"
                    name="rebootBehavior"
                    checked={windowsUpdate.rebootBehavior === option.value}
                    onChange={() =>
                      dispatch({
                        type: 'UPDATE_WINDOWS_UPDATE',
                        payload: { rebootBehavior: option.value as 'AutoReboot' | 'ScheduleReboot' | 'NoReboot' },
                      })
                    }
                    className="w-5 h-5 text-blue-600"
                  />
                  <div>
                    <span className="font-medium text-gray-900">{option.label}</span>
                    <p className="text-sm text-gray-500">{option.description}</p>
                  </div>
                </label>
              ))}
            </div>
          </div>
        </>
      )}

      {!windowsUpdate.enabled && (
        <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-4">
          <p className="text-yellow-800">
            <strong>Note:</strong> Skipping Windows Update means the image will not include the latest
            security patches. Consider enabling at least security updates for production deployments.
          </p>
        </div>
      )}
    </div>
  );
}
