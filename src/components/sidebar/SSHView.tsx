import { useState, useEffect, useCallback } from 'react';
import {
  Plus,
  MonitorSmartphone,
  Trash2,
  Plug,
  X,
  Server,
  KeyRound,
  Loader2,
  CheckCircle,
  AlertTriangle,
  Shield,
  Smartphone,
  Wifi,
  Settings2,
  ChevronDown,
  ChevronRight,
  FileCode,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import type { SSHProfile, AuthType, KeySetupProgress } from '../../lib/ssh';
import { SERVER_CONFIG_FIELDS } from '../../lib/ssh';

interface SSHViewProps {
  onConnectSSH?: (profileId: string, terminalId: string) => void;
}

// Matches the Rust SSHConfigHost struct returned by `list_ssh_config_hosts`.
interface SSHConfigHost {
  alias: string;
  hostname: string | null;
  user: string | null;
  port: number | null;
  identity_file: string | null;
  proxy_jump: string | null;
  source_file: string;
}

export function SSHView({ onConnectSSH }: SSHViewProps) {
  const [profiles, setProfiles] = useState<SSHProfile[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [editProfile, setEditProfile] = useState<SSHProfile | null>(null);

  // Form state
  const [name, setName] = useState('');
  const [host, setHost] = useState('');
  const [user, setUser] = useState('');
  const [port, setPort] = useState('22');
  const [keyFile, setKeyFile] = useState('');
  const [authType, setAuthType] = useState<AuthType>('password');
  const [mfaMethod, setMfaMethod] = useState<string>('push');
  const [serverConfig, setServerConfig] = useState<Record<string, string>>({});
  const [showServerConfig, setShowServerConfig] = useState(false);

  // SSH key setup state
  const [keySetupPassword, setKeySetupPassword] = useState('');
  const [keySetupStatus, setKeySetupStatus] = useState<'idle' | 'working' | 'mfa_waiting' | 'success' | 'error'>('idle');
  const [keySetupMessage, setKeySetupMessage] = useState('');
  const [savedProfileId, setSavedProfileId] = useState<string | null>(null);

  // Available SSH keys from ~/.ssh/
  const [availableKeys, setAvailableKeys] = useState<string[]>([]);

  // Parsed entries from ~/.ssh/config — used to preload the form for
  // advanced users who already maintain a client config.
  const [configHosts, setConfigHosts] = useState<SSHConfigHost[]>([]);
  const [showConfigPicker, setShowConfigPicker] = useState(false);

  const loadProfiles = useCallback(async () => {
    try {
      const list = await invoke<SSHProfile[]>('list_ssh_profiles');
      setProfiles(list);
    } catch (err) {
      console.error('Failed to load SSH profiles:', err);
    }
  }, []);

  // Scan ~/.ssh/ for key files
  const loadAvailableKeys = useCallback(async () => {
    try {
      const homeDir = await invoke<string>('get_home_dir');
      const sshDir = `${homeDir}/.ssh`;
      const entries = await invoke<Array<{ name: string; path: string; is_dir: boolean; size: number; extension?: string }>>('list_directory', {
        path: sshDir,
        showHidden: false,
      });
      const skipFiles = new Set(['known_hosts', 'config', 'authorized_keys', 'known_hosts.old', 'environment']);
      const keys = entries
        .filter(e => !e.is_dir && !e.name.endsWith('.pub') && !skipFiles.has(e.name) && e.size > 0 && e.size < 20000)
        .map(e => e.path);
      setAvailableKeys(keys);
    } catch {
      setAvailableKeys([]);
    }
  }, []);

  // Load hosts defined in ~/.ssh/config so users can autofill the form.
  const loadConfigHosts = useCallback(async () => {
    try {
      const list = await invoke<SSHConfigHost[]>('list_ssh_config_hosts');
      setConfigHosts(list);
    } catch {
      setConfigHosts([]);
    }
  }, []);

  // Autofill the form from a parsed ssh_config entry. Preserves fields the
  // user may have already typed (we only fill empty/default slots).
  const applyConfigHost = useCallback((h: SSHConfigHost) => {
    const resolvedHost = h.hostname || h.alias;
    // Alias becomes the human-readable name if user hasn't set one.
    setName((prev) => prev || h.alias);
    setHost((prev) => (prev && prev !== '' ? prev : resolvedHost));
    setUser((prev) => (prev && prev !== '' ? prev : h.user || ''));
    setPort((prev) => (prev && prev !== '22' ? prev : String(h.port ?? 22)));
    if (h.identity_file) {
      setKeyFile(h.identity_file);
      setAvailableKeys((prev) =>
        prev.includes(h.identity_file!) ? prev : [h.identity_file!, ...prev],
      );
    }
    setShowConfigPicker(false);
  }, []);

  useEffect(() => {
    loadProfiles();
  }, [loadProfiles]);

  const resetForm = () => {
    setName('');
    setHost('');
    setUser('');
    setPort('22');
    setKeyFile('');
    setAuthType('password');
    setMfaMethod('push');
    setServerConfig({});
    setShowServerConfig(false);
    setEditProfile(null);
    setSavedProfileId(null);
    setKeySetupPassword('');
    setKeySetupStatus('idle');
    setKeySetupMessage('');
    setShowForm(false);
  };

  const handleSave = async (andConnect = false) => {
    if (!host || !user) return;

    // Strip empty values from server config before saving
    const cleanConfig: Record<string, string> = {};
    for (const [k, v] of Object.entries(serverConfig)) {
      if (v && v.trim()) cleanConfig[k] = v.trim();
    }

    const profile: SSHProfile = {
      id: editProfile?.id || savedProfileId || crypto.randomUUID(),
      name: name || `${user}@${host}`,
      host,
      user,
      port: parseInt(port) || 22,
      key_file: keyFile || null,
      use_agent: true,
      auth_type: authType,
      mfa_method: authType === 'duo_mfa' ? mfaMethod : null,
      use_control_master: true,
      server_config: cleanConfig,
    };

    try {
      await invoke('save_ssh_profile', { profile });
      await loadProfiles();
      resetForm();
      if (andConnect) {
        handleConnect(profile);
      }
    } catch (err) {
      console.error('Failed to save SSH profile:', err);
    }
  };

  const handleDelete = async (profileId: string) => {
    try {
      await invoke('delete_ssh_profile', { profileId });
      await loadProfiles();
    } catch (err) {
      console.error('Failed to delete SSH profile:', err);
    }
  };

  const handleConnect = async (profile: SSHProfile) => {
    const terminalId = crypto.randomUUID();

    // Test SSH connection with key before using -i flag
    let useKeyFile = false;
    if (profile.key_file) {
      try {
        await invoke<string>('test_ssh_connection', { profileId: profile.id });
        useKeyFile = true;
      } catch {
        // Key doesn't work — connect without it (user will see password prompt in terminal)
      }
    }

    let sshCmd = `ssh ${profile.user}@${profile.host} -p ${profile.port} -o ServerAliveInterval=30`;
    // Add ControlMaster args so the terminal becomes the master connection
    if (profile.use_control_master) {
      const homeDir = await invoke<string>('get_home_dir').catch(() => '/tmp');
      const sockPath = `${homeDir}/.operon/sockets/ctrl_${profile.host}_${profile.port}_${profile.user}`;
      sshCmd += ` -o ControlMaster=auto -o ControlPath=${sockPath} -o ControlPersist=4h`;
    }
    if (useKeyFile && profile.key_file) {
      sshCmd += ` -i "${profile.key_file}"`;
    }

    await emit('open-ssh-terminal', {
      terminalId,
      title: `SSH: ${profile.name}`,
      sshCommand: sshCmd,
      profileId: profile.id,
      profileName: profile.name,
    });
    onConnectSSH?.(profile.id, terminalId);

    // Auto-detect server config on first connect if not yet configured
    const hasConfig = profile.server_config && Object.values(profile.server_config).some(v => v?.trim());
    if (!hasConfig) {
      // Run in background after a short delay to let the SSH connection establish
      setTimeout(async () => {
        try {
          const detected = await invoke<Record<string, string>>('detect_server_config', { profileId: profile.id });
          if (Object.keys(detected).length > 0) {
            const updated: SSHProfile = {
              ...profile,
              server_config: { ...profile.server_config, ...detected },
            };
            await invoke('save_ssh_profile', { profile: updated });
            await loadProfiles();
          }
        } catch {
          // Detection failed silently — user can still configure manually
        }
      }, 5000);
    }
  };

  const handleEdit = (profile: SSHProfile) => {
    setEditProfile(profile);
    setName(profile.name);
    setHost(profile.host);
    setUser(profile.user);
    setPort(String(profile.port));
    setKeyFile(profile.key_file || '');
    setAuthType(profile.auth_type || 'password');
    setMfaMethod(profile.mfa_method || 'push');
    setServerConfig(profile.server_config || {});
    setShowServerConfig(Object.keys(profile.server_config || {}).length > 0);
    { setShowForm(true); loadAvailableKeys(); loadConfigHosts(); };
  };

  const handleKeySetup = async () => {
    if (!keySetupPassword || !host || !user) return;

    // Save profile first (so backend can find it)
    const profileId = editProfile?.id || savedProfileId || crypto.randomUUID();
    setSavedProfileId(profileId);
    const profile: SSHProfile = {
      id: profileId,
      name: name || `${user}@${host}`,
      host,
      user,
      port: parseInt(port) || 22,
      key_file: null,
      use_agent: true,
      auth_type: authType,
      mfa_method: authType === 'duo_mfa' ? mfaMethod : null,
      use_control_master: true,
      server_config: serverConfig,
    };
    try {
      await invoke('save_ssh_profile', { profile });
    } catch { /* profile may already exist */ }

    setKeySetupStatus('working');
    setKeySetupMessage('Generating key and connecting...');

    // Listen for progress events from the backend
    const unlisten = await listen<KeySetupProgress>(
      `ssh-key-setup-progress-${profileId}`,
      (event) => {
        const { stage, message } = event.payload;
        setKeySetupMessage(message);
        if (stage === 'mfa_waiting') {
          setKeySetupStatus('mfa_waiting');
        } else if (stage === 'error') {
          setKeySetupStatus('error');
        } else if (stage === 'done') {
          // Will be set to success when the promise resolves
        }
      }
    );

    try {
      const keyPath = await invoke<string>('setup_ssh_key', {
        profileId,
        password: keySetupPassword,
        mfaMethod: authType === 'duo_mfa' ? mfaMethod : null,
      });
      setKeyFile(keyPath);
      setKeySetupStatus('success');
      setKeySetupMessage(keyPath);
      setKeySetupPassword('');
      await loadProfiles();
    } catch (err) {
      setKeySetupStatus('error');
      setKeySetupMessage(`${err}`);
    } finally {
      unlisten();
    }
  };

  if (showForm) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
          <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
            {editProfile ? 'Edit Connection' : 'New Connection'}
          </span>
          <button
            onClick={resetForm}
            className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
          >
            <X className="w-3.5 h-3.5" />
          </button>
        </div>
        <div className="p-3 space-y-3 overflow-y-auto flex-1">
          {/* ~/.ssh/config picker — shown only if the file has parsed entries. */}
          {!editProfile && configHosts.length > 0 && (
            <div className="p-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg">
              <button
                onClick={() => setShowConfigPicker(v => !v)}
                className="w-full flex items-center gap-1.5 text-left"
              >
                {showConfigPicker ? (
                  <ChevronDown className="w-3 h-3 text-zinc-500" />
                ) : (
                  <ChevronRight className="w-3 h-3 text-zinc-500" />
                )}
                <FileCode className="w-3 h-3 text-blue-400" />
                <span className="text-xs text-zinc-300 font-medium">
                  Existing Hosts
                </span>
                <span className="ml-auto text-[10px] text-zinc-500">
                  {configHosts.length} found
                </span>
              </button>
              {showConfigPicker && (
                <div className="mt-2 max-h-52 overflow-y-auto border-t border-zinc-800 pt-1.5 -mx-0.5">
                  {configHosts.map(h => {
                    const resolvedHost = h.hostname || h.alias;
                    const detail = [
                      h.user ? `${h.user}@${resolvedHost}` : resolvedHost,
                      h.port && h.port !== 22 ? `:${h.port}` : '',
                    ].join('');
                    return (
                      <button
                        key={`${h.alias}-${h.source_file}`}
                        onClick={() => applyConfigHost(h)}
                        className="w-full text-left px-2 py-1 rounded hover:bg-zinc-700/60 transition-colors"
                        title={`${h.source_file}${h.proxy_jump ? ` · via ${h.proxy_jump}` : ''}${h.identity_file ? ` · ${h.identity_file}` : ''}`}
                      >
                        <div className="text-xs text-zinc-200 truncate">{h.alias}</div>
                        <div className="text-[10px] text-zinc-500 truncate">
                          {detail}
                          {h.identity_file && <span className="ml-1 text-amber-400/60">· key</span>}
                          {h.proxy_jump && <span className="ml-1 text-purple-400/60">· jump</span>}
                        </div>
                      </button>
                    );
                  })}
                </div>
              )}
              {!showConfigPicker && (
                <p className="mt-1.5 text-[10px] text-zinc-500">
                  Autofill from your saved SSH hosts.
                </p>
              )}
            </div>
          )}

          <div>
            <label className="block text-xs text-zinc-500 mb-1">Name (optional)</label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Server"
              className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
            />
          </div>
          <div>
            <label className="block text-xs text-zinc-500 mb-1">Host *</label>
            <input
              value={host}
              onChange={(e) => setHost(e.target.value)}
              placeholder="192.168.1.100 or hpc3.rcic.uci.edu"
              className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
            />
          </div>
          <div className="flex gap-2">
            <div className="flex-1">
              <label className="block text-xs text-zinc-500 mb-1">User *</label>
              <input
                value={user}
                onChange={(e) => setUser(e.target.value)}
                placeholder="root"
                className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
              />
            </div>
            <div className="w-20">
              <label className="block text-xs text-zinc-500 mb-1">Port</label>
              <input
                value={port}
                onChange={(e) => setPort(e.target.value)}
                placeholder="22"
                className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500"
              />
            </div>
          </div>

          {/* Server type selector */}
          <div className="border-t border-zinc-800 pt-3">
            <label className="block text-xs text-zinc-500 mb-2">Server Authentication</label>
            <div className="flex gap-1.5">
              <button
                onClick={() => setAuthType('password')}
                className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded text-[11px] transition-colors ${
                  authType === 'password'
                    ? 'bg-blue-600/20 border border-blue-500/50 text-blue-300'
                    : 'bg-zinc-800 border border-zinc-700 text-zinc-400 hover:text-zinc-300'
                }`}
              >
                <KeyRound className="w-3 h-3" />
                Password
              </button>
              <button
                onClick={() => setAuthType('duo_mfa')}
                className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded text-[11px] transition-colors ${
                  authType === 'duo_mfa'
                    ? 'bg-purple-600/20 border border-purple-500/50 text-purple-300'
                    : 'bg-zinc-800 border border-zinc-700 text-zinc-400 hover:text-zinc-300'
                }`}
              >
                <Smartphone className="w-3 h-3" />
                Duo / MFA
              </button>
            </div>

            {authType === 'duo_mfa' && (
              <div className="mt-2 p-2 bg-purple-950/20 border border-purple-800/20 rounded">
                <p className="text-[10px] text-purple-300/80 mb-1.5">
                  When prompted, Operon will auto-respond to Duo:
                </p>
                <div className="flex gap-1.5">
                  {(['push', 'phone', 'passcode'] as const).map((method) => (
                    <button
                      key={method}
                      onClick={() => setMfaMethod(method)}
                      className={`flex-1 px-2 py-1 rounded text-[10px] transition-colors ${
                        mfaMethod === method
                          ? 'bg-purple-600/30 text-purple-200'
                          : 'bg-zinc-800/50 text-zinc-500 hover:text-zinc-400'
                      }`}
                    >
                      {method === 'push' ? 'Push' : method === 'phone' ? 'Phone Call' : 'Passcode'}
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>

          {/* Authentication / Key section */}
          <div className="border-t border-zinc-800 pt-3">
            <label className="block text-xs text-zinc-500 mb-2">SSH Key</label>

            {keySetupStatus === 'success' ? (
              <div className="flex items-center gap-2 p-2.5 bg-green-950/30 border border-green-800/30 rounded-lg mb-2">
                <CheckCircle className="w-4 h-4 text-green-400 shrink-0" />
                <div>
                  <p className="text-xs text-green-300 font-medium">SSH key installed!</p>
                  <p className="text-[10px] text-zinc-500">{keySetupMessage}</p>
                </div>
              </div>
            ) : keySetupStatus === 'mfa_waiting' ? (
              <div className="flex items-center gap-2 p-3 bg-purple-950/30 border border-purple-800/30 rounded-lg mb-2">
                <div className="relative">
                  <Smartphone className="w-5 h-5 text-purple-400" />
                  <span className="absolute -top-0.5 -right-0.5 w-2 h-2 bg-purple-400 rounded-full animate-ping" />
                </div>
                <div>
                  <p className="text-xs text-purple-300 font-medium">Waiting for Duo approval...</p>
                  <p className="text-[10px] text-zinc-500">{keySetupMessage}</p>
                </div>
              </div>
            ) : keySetupStatus === 'working' ? (
              <div className="flex items-center gap-2 p-3 bg-zinc-800 rounded-lg mb-2">
                <Loader2 className="w-4 h-4 text-amber-400 animate-spin shrink-0" />
                <p className="text-xs text-zinc-400">{keySetupMessage}</p>
              </div>
            ) : (
              <>
                {/* Option 1: Pick existing key */}
                <div className="mb-2">
                  <select
                    value={keyFile}
                    onChange={(e) => setKeyFile(e.target.value)}
                    className="w-full px-2.5 py-1.5 bg-zinc-800 border border-zinc-700 rounded text-sm text-zinc-100 outline-none focus:border-blue-500"
                  >
                    <option value="">No key selected (generate below)</option>
                    {availableKeys.map(k => {
                      const shortName = k.split('/').pop() || k;
                      return <option key={k} value={k}>{shortName}</option>;
                    })}
                  </select>
                </div>

                {/* Option 2: Generate key with password (+ optional Duo) */}
                {!keyFile && host && user && (
                  <div className="p-2.5 bg-zinc-800/50 border border-zinc-700/50 rounded-lg space-y-2">
                    <div className="flex items-center gap-1.5">
                      <KeyRound className="w-3 h-3 text-amber-400" />
                      <span className="text-[11px] text-zinc-300 font-medium">No key? Generate one automatically</span>
                    </div>
                    <p className="text-[10px] text-zinc-500">
                      {authType === 'duo_mfa'
                        ? 'Enter your password — Operon will handle Duo, install an SSH key, and skip MFA on future connections.'
                        : 'Enter your server password once — Operon will create an SSH key and install it.'}
                    </p>
                    <input
                      type="password"
                      value={keySetupPassword}
                      onChange={(e) => setKeySetupPassword(e.target.value)}
                      placeholder="Server password"
                      onKeyDown={(e) => { if (e.key === 'Enter') handleKeySetup(); }}
                      className="w-full px-2.5 py-1.5 bg-zinc-900 border border-zinc-700 rounded text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-amber-500"
                    />
                    {keySetupStatus === 'error' && (
                      <div className="flex items-start gap-1.5 text-[10px] text-red-300">
                        <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                        {keySetupMessage}
                      </div>
                    )}
                    <button
                      onClick={handleKeySetup}
                      disabled={!keySetupPassword}
                      className="w-full flex items-center justify-center gap-1.5 px-2.5 py-1.5 bg-amber-600 hover:bg-amber-700 disabled:bg-zinc-700 rounded text-xs text-white transition-colors"
                    >
                      <KeyRound className="w-3 h-3" />
                      Generate & Install Key
                    </button>
                    <p className="text-[9px] text-zinc-600">Password is used once and never stored.</p>
                  </div>
                )}
              </>
            )}
          </div>

          {/* Server Configuration (SLURM, conda, paths) */}
          <div className="border-t border-zinc-800 pt-3">
            <button
              onClick={() => setShowServerConfig(!showServerConfig)}
              className="flex items-center gap-1.5 w-full text-left"
            >
              {showServerConfig ? (
                <ChevronDown className="w-3 h-3 text-zinc-500" />
              ) : (
                <ChevronRight className="w-3 h-3 text-zinc-500" />
              )}
              <Settings2 className="w-3 h-3 text-cyan-400" />
              <span className="text-xs text-zinc-400 font-medium">Server Configuration</span>
              {Object.values(serverConfig).filter(v => v?.trim()).length > 0 && (
                <span className="ml-auto text-[9px] text-cyan-400/70 bg-cyan-400/10 px-1.5 py-0.5 rounded">
                  {Object.values(serverConfig).filter(v => v?.trim()).length} set
                </span>
              )}
            </button>

            {showServerConfig && (
              <div className="mt-2 space-y-3">
                <div className="flex items-center justify-between">
                  <p className="text-[10px] text-zinc-600">
                    Used by all protocols and scripts on this server.
                  </p>
                  {(editProfile || savedProfileId) && (
                    <button
                      onClick={async () => {
                        const pid = editProfile?.id || savedProfileId;
                        if (!pid) return;
                        try {
                          const detected = await invoke<Record<string, string>>('detect_server_config', { profileId: pid });
                          if (Object.keys(detected).length > 0) {
                            setServerConfig(prev => {
                              const merged = { ...prev };
                              for (const [k, v] of Object.entries(detected)) {
                                // Only fill in empty fields — don't overwrite user edits
                                if (!merged[k]?.trim()) merged[k] = v;
                              }
                              return merged;
                            });
                          }
                        } catch (err) {
                          console.error('Auto-detect failed:', err);
                        }
                      }}
                      className="flex items-center gap-1 px-2 py-0.5 text-[10px] text-cyan-400 hover:text-cyan-300 bg-cyan-400/10 hover:bg-cyan-400/20 rounded transition-colors"
                      title="Detect SLURM accounts, conda envs, and paths from this server"
                    >
                      <Wifi className="w-2.5 h-2.5" />
                      Auto-Detect
                    </button>
                  )}
                </div>

                {/* SLURM */}
                <div>
                  <span className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider">SLURM</span>
                  <div className="mt-1 space-y-1.5">
                    {SERVER_CONFIG_FIELDS.filter(f => f.group === 'slurm').map(field => (
                      <div key={field.key}>
                        <label className="block text-[10px] text-zinc-600 mb-0.5">{field.label}</label>
                        <input
                          value={serverConfig[field.key] || ''}
                          onChange={(e) => setServerConfig(prev => ({ ...prev, [field.key]: e.target.value }))}
                          placeholder={field.placeholder}
                          className="w-full px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-xs text-zinc-100 placeholder:text-zinc-700 outline-none focus:border-cyan-500"
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Environment */}
                <div>
                  <span className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider">Environment</span>
                  <div className="mt-1 space-y-1.5">
                    {SERVER_CONFIG_FIELDS.filter(f => f.group === 'environment').map(field => (
                      <div key={field.key}>
                        <label className="block text-[10px] text-zinc-600 mb-0.5">{field.label}</label>
                        <input
                          value={serverConfig[field.key] || ''}
                          onChange={(e) => setServerConfig(prev => ({ ...prev, [field.key]: e.target.value }))}
                          placeholder={field.placeholder}
                          className="w-full px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-xs text-zinc-100 placeholder:text-zinc-700 outline-none focus:border-cyan-500"
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Paths */}
                <div>
                  <span className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider">Paths</span>
                  <div className="mt-1 space-y-1.5">
                    {SERVER_CONFIG_FIELDS.filter(f => f.group === 'paths').map(field => (
                      <div key={field.key}>
                        <label className="block text-[10px] text-zinc-600 mb-0.5">{field.label}</label>
                        <input
                          value={serverConfig[field.key] || ''}
                          onChange={(e) => setServerConfig(prev => ({ ...prev, [field.key]: e.target.value }))}
                          placeholder={field.placeholder}
                          className="w-full px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-xs text-zinc-100 placeholder:text-zinc-700 outline-none focus:border-cyan-500"
                        />
                      </div>
                    ))}
                  </div>
                </div>

                {/* Custom key-value pairs */}
                <div>
                  <span className="text-[10px] text-zinc-500 font-medium uppercase tracking-wider">Custom</span>
                  <p className="text-[9px] text-zinc-600 mt-0.5 mb-1">Add any custom variables (available as {'{key}'} in scripts)</p>
                  {Object.entries(serverConfig)
                    .filter(([k]) => !SERVER_CONFIG_FIELDS.some(f => f.key === k))
                    .map(([key, value]) => (
                      <div key={key} className="flex gap-1 mb-1">
                        <input
                          value={key}
                          disabled
                          className="w-1/3 px-2 py-1 bg-zinc-900 border border-zinc-700 rounded text-[10px] text-zinc-400"
                        />
                        <input
                          value={value}
                          onChange={(e) => setServerConfig(prev => ({ ...prev, [key]: e.target.value }))}
                          className="flex-1 px-2 py-1 bg-zinc-800 border border-zinc-700 rounded text-xs text-zinc-100 outline-none focus:border-cyan-500"
                        />
                        <button
                          onClick={() => setServerConfig(prev => {
                            const next = { ...prev };
                            delete next[key];
                            return next;
                          })}
                          className="px-1.5 text-red-400 hover:text-red-300"
                        >
                          <X className="w-3 h-3" />
                        </button>
                      </div>
                    ))}
                  <button
                    onClick={() => {
                      const key = prompt('Variable name (e.g. email, pi_name):');
                      if (key && key.trim()) {
                        setServerConfig(prev => ({ ...prev, [key.trim().toLowerCase().replace(/\s+/g, '_')]: '' }));
                      }
                    }}
                    className="mt-1 text-[10px] text-cyan-400 hover:text-cyan-300"
                  >
                    + Add custom variable
                  </button>
                </div>
              </div>
            )}
          </div>

          <button
            onClick={() => handleSave(keySetupStatus === 'success')}
            disabled={!host || !user}
            className="w-full px-3 py-2 bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 rounded text-sm text-white transition-colors"
          >
            {editProfile ? 'Update Connection' : (keySetupStatus === 'success' ? 'Save & Connect' : 'Save Connection')}
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Remote SSH
        </span>
        <button
          onClick={() => { setShowForm(true); loadAvailableKeys(); loadConfigHosts(); }}
          className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300"
        >
          <Plus className="w-3.5 h-3.5" />
        </button>
      </div>

      {profiles.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center px-4 text-center">
          <MonitorSmartphone className="w-10 h-10 text-zinc-700 mb-3" />
          <p className="text-sm text-zinc-500 mb-1">No SSH connections</p>
          <p className="text-xs text-zinc-600">Add a remote server to connect via SSH</p>
          <button
            onClick={() => { setShowForm(true); loadAvailableKeys(); loadConfigHosts(); }}
            className="mt-3 px-3 py-1.5 bg-blue-600 hover:bg-blue-700 rounded text-xs text-white transition-colors"
          >
            Add Connection
          </button>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto py-1">
          {profiles.map((profile) => (
            <div
              key={profile.id}
              className="group px-3 py-2 hover:bg-zinc-800/50 cursor-pointer border-b border-zinc-800/30"
              onDoubleClick={() => handleEdit(profile)}
            >
              <div className="flex items-center gap-2">
                <Server className="w-4 h-4 text-zinc-500 shrink-0" />
                <div className="flex-1 min-w-0">
                  <div className="text-sm text-zinc-300 truncate">{profile.name}</div>
                  <div className="text-xs text-zinc-600 truncate">
                    {profile.user}@{profile.host}:{profile.port}
                    {profile.server_config && Object.keys(profile.server_config).length > 0 && (
                      <span className="ml-1.5 text-[9px] text-cyan-400/60">
                        {profile.server_config.slurm_account || `${Object.keys(profile.server_config).length} settings`}
                      </span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    onClick={() => handleConnect(profile)}
                    className="p-1 rounded hover:bg-zinc-700 text-green-500"
                    title="Connect"
                  >
                    <Plug className="w-3.5 h-3.5" />
                  </button>
                  <button
                    onClick={() => handleDelete(profile.id)}
                    className="p-1 rounded hover:bg-zinc-700 text-red-500"
                    title="Delete"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              {/* Auth status indicator */}
              <div className="flex items-center gap-1.5 mt-1 ml-6">
                {profile.key_file ? (
                  <span className="flex items-center gap-1 text-[10px] text-green-400/70">
                    <Shield className="w-2.5 h-2.5" />
                    Key auth
                    {profile.auth_type === 'duo_mfa' && (
                      <span className="text-purple-400/60 ml-1">+ ControlMaster</span>
                    )}
                  </span>
                ) : profile.auth_type === 'duo_mfa' ? (
                  <span className="flex items-center gap-1 text-[10px] text-purple-400/60">
                    <Smartphone className="w-2.5 h-2.5" />
                    Duo MFA
                  </span>
                ) : (
                  <span className="flex items-center gap-1 text-[10px] text-amber-400/60">
                    <KeyRound className="w-2.5 h-2.5" />
                    Password auth
                  </span>
                )}
              </div>

              {/* Server config summary */}
              {profile.server_config && Object.keys(profile.server_config).length > 0 && (
                <div className="flex flex-wrap gap-1 mt-1 ml-6">
                  {profile.server_config.slurm_account && (
                    <span className="text-[9px] bg-cyan-400/10 text-cyan-400/70 px-1.5 py-0.5 rounded">
                      {profile.server_config.slurm_account}
                    </span>
                  )}
                  {profile.server_config.conda_env && (
                    <span className="text-[9px] bg-green-400/10 text-green-400/70 px-1.5 py-0.5 rounded">
                      {profile.server_config.conda_env}
                    </span>
                  )}
                  {profile.server_config.slurm_gpu_partition && (
                    <span className="text-[9px] bg-purple-400/10 text-purple-400/70 px-1.5 py-0.5 rounded">
                      GPU: {profile.server_config.slurm_gpu_partition}
                    </span>
                  )}
                  {!profile.server_config.slurm_account && !profile.server_config.conda_env && (
                    <span className="text-[9px] text-zinc-600">
                      <Settings2 className="w-2.5 h-2.5 inline mr-0.5" />
                      {Object.keys(profile.server_config).length} settings
                    </span>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

    </div>
  );
}
