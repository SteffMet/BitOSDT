import { useState } from 'react';
import { AppModal } from '../shared/AppModal';
import { useWizard } from './WizardContext';
import { UserAccount } from './types';

function validateComputerName(value: string): string | null {
  const trimmed = value.trim();
  if (!trimmed || trimmed === '*') {
    return null;
  }
  if (trimmed.length > 15) {
    return 'Computer name must be 1-15 characters.';
  }
  if (trimmed.startsWith('-') || trimmed.endsWith('-')) {
    return 'Computer name cannot start or end with a hyphen.';
  }
  if (!/^[A-Za-z0-9-]+$/.test(trimmed)) {
    return 'Computer name can only contain letters, numbers, and hyphens.';
  }
  return null;
}

export function StepOobeUsers() {
  const { state, dispatch } = useWizard();
  const { oobeConfig, userAccounts } = state;
  const computerNameError = validateComputerName(oobeConfig.computerName);
  const hasLocalAdministratorUser = userAccounts.some((user) => user.group === 'Administrators');

  const [showAddUser, setShowAddUser] = useState(false);
  const [newUser, setNewUser] = useState<UserAccount>({
    username: '',
    password: '',
    displayName: '',
    group: 'Administrators',
    passwordNeverExpires: true,
    requirePasswordChange: false,
  });

  const handleAddUser = () => {
    if (newUser.username && newUser.password) {
      dispatch({ type: 'ADD_USER', payload: newUser });
      setNewUser({
        username: '',
        password: '',
        displayName: '',
        group: 'Administrators',
        passwordNeverExpires: true,
        requirePasswordChange: false,
      });
      setShowAddUser(false);
    }
  };

  return (
    <div className="wizard-step space-y-8">
      {/* OOBE Configuration */}
      <div>
        <h2 className="text-2xl font-bold text-gray-900 mb-2">OOBE Configuration</h2>
        <p className="text-gray-600 mb-4">
          Configure what parts of the Out-of-Box Experience to skip.
        </p>

        <div className="bg-gray-50 rounded-lg p-6 space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            <label className="flex items-center space-x-3 p-3 bg-white rounded-lg border border-gray-200 hover:border-blue-300 transition-colors cursor-pointer">
              <input
                type="checkbox"
                checked={oobeConfig.skipMachineOobe}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_OOBE', payload: { skipMachineOobe: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900 font-medium">Skip Machine OOBE</span>
            </label>

            <label className="flex items-center space-x-3 p-3 bg-white rounded-lg border border-gray-200 hover:border-blue-300 transition-colors cursor-pointer">
              <input
                type="checkbox"
                checked={oobeConfig.skipUserOobe}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_OOBE', payload: { skipUserOobe: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900 font-medium">Skip User OOBE</span>
            </label>

            <label className="flex items-center space-x-3 p-3 bg-white rounded-lg border border-gray-200 hover:border-blue-300 transition-colors cursor-pointer">
              <input
                type="checkbox"
                checked={oobeConfig.hideEula}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_OOBE', payload: { hideEula: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900 font-medium">Accept EULA Automatically</span>
            </label>

            <label className="flex items-center space-x-3 p-3 bg-white rounded-lg border border-gray-200 hover:border-blue-300 transition-colors cursor-pointer">
              <input
                type="checkbox"
                checked={oobeConfig.hideWirelessSetup}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_OOBE', payload: { hideWirelessSetup: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900 font-medium">Hide Wireless Setup</span>
            </label>

            <label className="flex items-center space-x-3 p-3 bg-white rounded-lg border border-gray-200 hover:border-blue-300 transition-colors cursor-pointer">
              <input
                type="checkbox"
                checked={oobeConfig.hideOnlineAccountScreens}
                onChange={(e) =>
                  dispatch({ type: 'UPDATE_OOBE', payload: { hideOnlineAccountScreens: e.target.checked } })
                }
                className="w-5 h-5 text-blue-600 rounded"
              />
              <span className="text-gray-900 font-medium">Hide Online Account Screens</span>
            </label>
          </div>

          {oobeConfig.skipUserOobe && !hasLocalAdministratorUser && (
            <div className="rounded-lg border border-amber-300 bg-amber-50 p-4">
              <p className="text-sm font-medium text-amber-900">
                Skip User OOBE requires at least one local administrator account so the deployed
                device still has a usable sign-in path.
              </p>
            </div>
          )}

          <div className="pt-4 border-t border-gray-200">
            <label className="block text-sm font-medium text-gray-700 mb-2">
              Network Location
            </label>
            <select
              value={oobeConfig.networkLocation}
              onChange={(e) =>
                dispatch({
                  type: 'UPDATE_OOBE',
                  payload: { networkLocation: e.target.value as 'Home' | 'Work' | 'Other' },
                })
              }
              className="px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
            >
              <option value="Work">Work</option>
              <option value="Home">Home</option>
              <option value="Other">Other</option>
            </select>
          </div>

          <div className="pt-4 border-t border-gray-200">
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Computer Name
            </label>
            <input
              type="text"
              value={oobeConfig.computerName}
              onChange={(e) =>
                dispatch({ type: 'UPDATE_OOBE', payload: { computerName: e.target.value } })
              }
              placeholder="Leave blank or use * for auto-generated name"
              className={`w-full md:w-[26rem] px-4 py-2 border rounded-lg text-gray-900 ${
                computerNameError ? 'border-red-400' : 'border-gray-300'
              }`}
              maxLength={15}
            />
            <p className="text-xs text-gray-500 mt-1">
              Leave blank or use <code>*</code> to auto-generate.
            </p>
            {computerNameError && (
              <p className="text-sm text-red-700 mt-2">{computerNameError}</p>
            )}
          </div>
        </div>
      </div>

      {/* User Accounts */}
      <div>
        <div className="flex justify-between items-center mb-4">
          <div>
            <h2 className="text-2xl font-bold text-gray-900">User Accounts</h2>
            <p className="text-gray-600">Create local user accounts during deployment.</p>
          </div>
          <button
            onClick={() => setShowAddUser(true)}
            className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700"
          >
            Add User
          </button>
        </div>

        {/* User List */}
        {userAccounts.length > 0 ? (
          <div className="space-y-3">
            {userAccounts.map((user, index) => (
              <div
                key={index}
                className="flex items-center justify-between bg-gray-50 rounded-lg p-4"
              >
                <div>
                  <p className="font-medium text-gray-900">{user.username}</p>
                  <p className="text-sm text-gray-500">
                    {user.group} {user.displayName && `• ${user.displayName}`}
                  </p>
                </div>
                <button
                  onClick={() => dispatch({ type: 'REMOVE_USER', index })}
                  className="text-red-600 hover:text-red-800"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        ) : (
          <div className="text-center py-8 bg-gray-50 rounded-lg">
            <p className="text-gray-500">No users added yet.</p>
            <p className="text-sm text-gray-400">Click "Add User" to create a local account.</p>
          </div>
        )}

        {/* Add User Modal */}
        {showAddUser && (
          <AppModal open onClose={() => setShowAddUser(false)} size="compact" labelledBy="add-user-title">
            <>
              <div className="ops-modal-head">
                <div>
                  <h3 id="add-user-title" className="ops-card-title">Add User Account</h3>
                </div>
              </div>
              <div className="ops-modal-body">
              <div className="space-y-4">
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Username *</label>
                  <input
                    type="text"
                    value={newUser.username}
                    onChange={(e) => setNewUser({ ...newUser, username: e.target.value })}
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                    placeholder="Enter username"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Password *</label>
                  <input
                    type="password"
                    value={newUser.password}
                    onChange={(e) => setNewUser({ ...newUser, password: e.target.value })}
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                    placeholder="Enter password"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Display Name</label>
                  <input
                    type="text"
                    value={newUser.displayName}
                    onChange={(e) => setNewUser({ ...newUser, displayName: e.target.value })}
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                    placeholder="Optional display name"
                  />
                </div>
                <div>
                  <label className="block text-sm font-medium text-gray-700 mb-1">Group</label>
                  <select
                    value={newUser.group}
                    onChange={(e) =>
                      setNewUser({ ...newUser, group: e.target.value as 'Administrators' | 'Users' })
                    }
                    className="w-full px-4 py-2 border border-gray-300 rounded-lg text-gray-900"
                  >
                    <option value="Administrators">Administrators</option>
                    <option value="Users">Users</option>
                  </select>
                </div>
                <label className="flex items-center space-x-3">
                  <input
                    type="checkbox"
                    checked={newUser.passwordNeverExpires}
                    onChange={(e) =>
                      setNewUser({ ...newUser, passwordNeverExpires: e.target.checked })
                    }
                    className="w-5 h-5 text-blue-600 rounded"
                  />
                  <span className="text-gray-900">Password never expires</span>
                </label>
              </div>
              </div>
              <div className="ops-modal-foot">
                <button
                  onClick={() => setShowAddUser(false)}
                  className="ops-btn ops-btn-ghost"
                >
                  Cancel
                </button>
                <button
                  onClick={handleAddUser}
                  disabled={!newUser.username || !newUser.password}
                  className="ops-btn ops-btn-primary"
                >
                  Add User
                </button>
              </div>
            </>
          </AppModal>
        )}
      </div>
    </div>
  );
}
