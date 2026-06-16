import { useState, useEffect, useCallback, useRef } from 'react';
import {
  CheckCircle,
  XCircle,
  Loader2,
  Terminal,
  Globe,
  ArrowRight,
  ArrowLeft,
  Download,
  Sparkles,
  AlertTriangle,
  Key,
  LogIn,
  Server,
  ClipboardList,
  Bot,
  Code2,
  MonitorSmartphone,
  FolderTree,
  MessageSquare,
  Keyboard,
  Zap,
  GitBranch,
  Mic,
  BookMarked,
  Package,
  Settings2,
  FileText,
  FlaskConical,
  BookOpen,
  AtSign,
  Pencil,
  Save,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { MCPCatalogEntry } from '../../types/mcp';
import { getMCPCatalog, installMCPServer, checkMCPDependencies } from '../../lib/mcp';
import { checkSshAvailable, type SshStatus } from '../../lib/claude';
import { modKey, isMac, isWindows, isLinux } from '../../lib/platform';

interface DependencyStatus {
  xcode_cli: boolean;
  node: boolean;
  node_version: string | null;
  npm: boolean;
  npm_version: string | null;
  claude_code: boolean;
  claude_version: string | null;
  git_bash: boolean;
}

interface InstallProgress {
  step: string;    // "xcode" | "homebrew" | "node" | "gh" | "claude" | "done" | "error"
  status: string;  // "starting" | "downloading" | "installing" | "waiting" | "complete" | "skipped" | "error"
  message: string;
  percent: number; // 0-100
}

interface SetupWizardProps {
  onComplete: () => void;
  mode?: 'fullscreen' | 'modal';
}

type Step = 'welcome' | 'checking' | 'install-xcode' | 'install-tools' | 'install-claude' | 'installing' | 'dependencies' | 'auth' | 'research-tools' | 'tour-overview' | 'tour-modes' | 'tour-remote' | 'tour-features' | 'tour-shortcuts' | 'complete';

function ResearchToolsStep({ onContinue }: { onContinue: () => void }) {
  const [catalog, setCatalog] = useState<MCPCatalogEntry[]>([]);
  const [enabled, setEnabled] = useState<Record<string, boolean>>({});
  const [installing, setInstalling] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getMCPCatalog().then(setCatalog).catch(console.error);
  }, []);

  const handleToggle = async (entry: MCPCatalogEntry) => {
    if (enabled[entry.id]) {
      setEnabled(prev => ({ ...prev, [entry.id]: false }));
      return;
    }
    setInstalling(entry.id);
    setError(null);
    try {
      const dep = await checkMCPDependencies(entry.config.name);
      if (dep.satisfied) {
        await installMCPServer(entry.id);
        setEnabled(prev => ({ ...prev, [entry.id]: true }));
      } else {
        setError(`${entry.runtime === 'node' ? 'Node.js' : 'Python'} not found. ${dep.install_hint}`);
      }
    } catch (e) {
      setError(String(e));
    }
    setInstalling(null);
  };

  return (
    <div className="space-y-5">
      <div className="text-center">
        <div className="w-12 h-12 rounded-xl bg-teal-900/30 flex items-center justify-center mx-auto mb-3">
          <Server className="w-6 h-6 text-teal-600 dark:text-teal-400" />
        </div>
        <h2 className="text-lg font-semibold text-primary">Research Tools</h2>
        <p className="text-muted text-sm mt-1">
          Enable MCP servers to give Claude access to scientific databases. This is optional — you can change it later in Settings.
        </p>
      </div>

      {error && (
        <div className="flex items-center gap-2 p-2.5 bg-yellow-950/20 border border-yellow-800/30 rounded-lg">
          <AlertTriangle className="w-3.5 h-3.5 text-yellow-600 dark:text-yellow-400 shrink-0" />
          <span className="text-[11px] text-yellow-700 dark:text-yellow-300">{error}</span>
        </div>
      )}

      <div className="space-y-2.5">
        {catalog.map((entry) => (
          <div key={entry.id} className="p-3.5 bg-panel rounded-lg border border-border-default">
            <div className="flex items-start gap-3">
              <div className={`w-9 h-9 rounded-lg flex items-center justify-center shrink-0 mt-0.5 ${
                entry.runtime === 'node' ? 'bg-violet-900/30' : 'bg-teal-900/30'
              }`}>
                <Server className={`w-4 h-4 ${entry.runtime === 'node' ? 'text-violet-600 dark:text-violet-400' : 'text-teal-600 dark:text-teal-400'}`} />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="text-sm font-medium text-primary">{entry.name}</span>
                  <span className="text-[10px] text-subtle bg-surface px-1.5 py-0.5 rounded">
                    {entry.runtime === 'node' ? 'Node.js' : 'Python'}
                  </span>
                  <span className="text-[10px] text-subtle">{entry.tools_count} tools</span>
                </div>
                <p className="text-[11px] text-muted mt-1 leading-relaxed">{entry.description}</p>
                {entry.databases.length > 0 && (
                  <p className="text-[10px] text-subtle mt-1">
                    Databases: {entry.databases.slice(0, 5).join(', ')}{entry.databases.length > 5 ? ` +${entry.databases.length - 5} more` : ''}
                  </p>
                )}
              </div>
              <div className="shrink-0">
                {installing === entry.id ? (
                  <Loader2 className="w-4 h-4 text-blue-600 dark:text-blue-400 animate-spin" />
                ) : (
                  <button
                    onClick={() => handleToggle(entry)}
                    className={`relative w-9 h-5 rounded-full transition-colors ${
                      enabled[entry.id] ? 'bg-blue-600' : 'bg-elevated'
                    }`}
                  >
                    <span className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                      enabled[entry.id] ? 'translate-x-4' : 'translate-x-0.5'
                    }`} />
                  </button>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>

      <button
        onClick={onContinue}
        className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"
      >
        Continue
        <ArrowRight className="w-4 h-4" />
      </button>
    </div>
  );
}

export function SetupWizard({ onComplete, mode = 'fullscreen' }: SetupWizardProps) {
  const [step, setStep] = useState<Step>('welcome');
  const [deps, setDeps] = useState<DependencyStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [apiKey, setApiKey] = useState('');
  const [authMethod, setAuthMethod] = useState<'api' | 'oauth' | null>(null);
  const [oauthState, setOauthState] = useState<'idle' | 'launched' | 'checking' | 'success' | 'failed'>('idle');
  const [oauthMessage, setOauthMessage] = useState<string | null>(null);

  // Install progress state
  const [installPercent, setInstallPercent] = useState(0);
  const [installMessage, setInstallMessage] = useState('');
  const [installSteps, setInstallSteps] = useState<Record<string, { status: string; message: string }>>({});
  const [installDone, setInstallDone] = useState(false);
  const [installHadErrors, setInstallHadErrors] = useState(false);
  const unlistenRef = useRef<(() => void) | null>(null);

  // Phase-specific install state
  const [phaseRunning, setPhaseRunning] = useState(false);
  const [phaseDone, setPhaseDone] = useState(false);
  const [phaseError, setPhaseError] = useState<string | null>(null);

  // Track which steps completed with errors (for StepIndicator coloring)
  const [failedSteps, setFailedSteps] = useState<Set<Step>>(new Set());

  // OpenSSH client availability (Windows only — may be disabled by default)
  const [sshStatus, setSshStatus] = useState<SshStatus | null>(null);

  // Step indicator — Xcode is macOS-only
  const allSteps: { key: Step; label: string }[] = [
    { key: 'welcome', label: 'Welcome' },
    ...(isMac ? [{ key: 'install-xcode' as Step, label: 'Xcode' }] : []),
    { key: 'install-tools', label: 'Tools' },
    { key: 'install-claude', label: 'Claude' },
    { key: 'auth', label: 'Auth' },
    { key: 'research-tools', label: 'Research' },
    { key: 'tour-overview', label: 'Tour' },
    { key: 'complete', label: 'Ready' },
  ];

  const currentStepIndex = (() => {
    // On non-macOS, Xcode step is removed so indices shift down by 1
    const xcodeOffset = isMac ? 0 : -1;
    if (step === 'welcome') return 0;
    if (step === 'checking' || step === 'install-xcode') return 1; // Only reached on macOS
    if (step === 'install-tools' || step === 'installing') return 2 + xcodeOffset;
    if (step === 'install-claude') return 3 + xcodeOffset;
    if (step === 'auth') return 4 + xcodeOffset;
    if (step === 'research-tools') return 5 + xcodeOffset;
    if (step === 'tour-overview' || step === 'tour-modes' || step === 'tour-remote' || step === 'tour-features' || step === 'tour-shortcuts') return 6 + xcodeOffset;
    if (step === 'complete') return 7 + xcodeOffset;
    return 0;
  })();

  // Check dependencies
  const checkDeps = useCallback(async () => {
    setStep('checking');
    setError(null);
    try {
      const status = await invoke<DependencyStatus>('check_local_dependencies');
      setDeps(status);
      const allGood = status.xcode_cli && status.node && status.claude_code
        && (isWindows ? status.git_bash : true);
      if (allGood) {
        // Everything's already installed — skip to auth
        setStep('auth');
      } else if (isMac && !status.xcode_cli) {
        // Start with Xcode (macOS only)
        setStep('install-xcode');
      } else if (!status.node || (isWindows && !status.git_bash)) {
        // Need tools (Node.js, package manager, Git on Windows)
        setStep('install-tools');
      } else {
        // Just need Claude Code
        setStep('install-claude');
      }
    } catch (e) {
      setError(`Failed to check dependencies: ${e}`);
      setStep('dependencies');
    }
  }, []);

  // Start the automatic background installation
  const startInstall = async () => {
    setStep('installing');
    setInstallPercent(0);
    setInstallMessage('Preparing installation...');
    setInstallSteps({});
    setInstallDone(false);
    setInstallHadErrors(false);

    // Listen for progress events from the backend
    const unlisten = await listen<InstallProgress>('install-progress', (event) => {
      const { step: iStep, status, message, percent } = event.payload;

      setInstallPercent(percent);
      setInstallMessage(message);

      setInstallSteps(prev => ({
        ...prev,
        [iStep]: { status, message },
      }));

      if (iStep === 'done') {
        setInstallDone(true);
        if (status === 'error') {
          setInstallHadErrors(true);
        }
      }
    });

    unlistenRef.current = unlisten;

    try {
      await invoke('install_all_dependencies');
    } catch (e) {
      setError(`Installation failed: ${e}`);
      setInstallHadErrors(true);
      setInstallDone(true);
    }
  };

  // Run a single install phase and listen for progress events
  const runPhase = async (command: string) => {
    setPhaseRunning(true);
    setPhaseDone(false);
    setPhaseError(null);
    setInstallSteps({});
    setInstallPercent(0);
    setInstallMessage('Preparing...');

    const unlisten = await listen<InstallProgress>('install-progress', (event) => {
      const { step: iStep, status, message, percent } = event.payload;
      setInstallPercent(percent);
      setInstallMessage(message);
      setInstallSteps(prev => ({ ...prev, [iStep]: { status, message } }));
    });
    unlistenRef.current = unlisten;

    try {
      const allOk = await invoke<boolean>(command);
      // allOk is false when some tools failed (but the command itself didn't throw)
      if (!allOk) {
        setPhaseError('Some tools could not be installed automatically.');
      }
      setPhaseDone(true);
      setPhaseRunning(false);
    } catch (e) {
      setPhaseError(String(e));
      setPhaseRunning(false);
      setPhaseDone(true);
    } finally {
      unlisten();
      unlistenRef.current = null;
    }
  };

  // Run a phase and track which wizard step it belongs to for error display
  const runPhaseForStep = async (command: string, wizardStep: Step) => {
    await runPhase(command);
    // After the phase finishes, re-check deps to see what actually succeeded
    try {
      const status = await invoke<DependencyStatus>('check_local_dependencies');
      setDeps(status);
      // On Windows, if Git Bash is still missing after tools phase, mark as failed
      const criticalMissing = isWindows
        ? (!status.node || !status.git_bash)
        : !status.node;
      if (criticalMissing) {
        setFailedSteps(prev => new Set([...prev, wizardStep]));
      } else {
        setFailedSteps(prev => { const next = new Set(prev); next.delete(wizardStep); return next; });
      }
    } catch { /* ignore re-check failures */ }
  };

  const startXcodeInstall = () => runPhaseForStep('install_phase_xcode', 'install-xcode' as Step);
  const startToolsInstall = () => runPhaseForStep('install_phase_tools', 'install-tools' as Step);
  const startClaudeInstall = () => runPhaseForStep('install_phase_claude', 'install-claude' as Step);

  // Cleanup listener on unmount
  useEffect(() => {
    return () => {
      if (unlistenRef.current) {
        unlistenRef.current();
      }
    };
  }, []);

  // Auto-advance after legacy install completes
  useEffect(() => {
    if (installDone && !installHadErrors && step === 'installing') {
      const timer = setTimeout(() => setStep('auth'), 1200);
      return () => clearTimeout(timer);
    }
  }, [installDone, installHadErrors, step]);

  // Check OpenSSH client availability when reaching the tools page (Windows only).
  // Windows 10/11 ship an OpenSSH client but it can be disabled.
  useEffect(() => {
    if (isWindows && step === 'install-tools' && sshStatus === null) {
      checkSshAvailable().then(setSshStatus).catch(() => {});
    }
  }, [step, sshStatus]);

  // Reset phase state when navigating between install pages
  useEffect(() => {
    if (step === 'install-xcode' || step === 'install-tools' || step === 'install-claude') {
      setPhaseRunning(false);
      setPhaseDone(false);
      setPhaseError(null);
      setInstallSteps({});
      setInstallPercent(0);
      setInstallMessage('');
    }
  }, [step]);

  // Launch OAuth login flow
  const oauthPollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const launchOAuth = async () => {
    setOauthState('launched');
    setOauthMessage(null);
    setError(null);
    try {
      const msg = await invoke<string>('launch_claude_login');
      setOauthMessage(msg);

      // Auto-poll for auth status every 3 seconds
      if (oauthPollRef.current) clearInterval(oauthPollRef.current);
      oauthPollRef.current = setInterval(async () => {
        try {
          const ok = await invoke<boolean>('check_oauth_status');
          if (ok) {
            if (oauthPollRef.current) clearInterval(oauthPollRef.current);
            oauthPollRef.current = null;
            setOauthState('success');
            setTimeout(() => setStep('research-tools'), 600);
          }
        } catch { /* ignore poll errors */ }
      }, 3000);
    } catch (e) {
      setError(`Failed to launch login: ${e}`);
      setOauthState('failed');
    }
  };

  // Clean up poll on unmount
  useEffect(() => {
    return () => {
      if (oauthPollRef.current) clearInterval(oauthPollRef.current);
    };
  }, []);

  // Verify OAuth succeeded (manual button)
  const verifyOAuth = async () => {
    setOauthState('checking');
    setError(null);
    try {
      const ok = await invoke<boolean>('check_oauth_status');
      if (ok) {
        if (oauthPollRef.current) clearInterval(oauthPollRef.current);
        oauthPollRef.current = null;
        setOauthState('success');
        setTimeout(() => setStep('research-tools'), 600);
      } else {
        setOauthState('failed');
        setError('No OAuth credentials found yet. Complete the login in your browser, then try again.');
      }
    } catch (e) {
      setOauthState('failed');
      setError(`Verification failed: ${e}`);
    }
  };

  // Save API key and proceed to research tools
  const completeAuth = async (skipAuth = false) => {
    if (!skipAuth && apiKey.trim()) {
      try {
        await invoke('store_api_key', { key: apiKey.trim() });
      } catch (e) {
        setError(`Failed to store API key: ${e}`);
        return;
      }
    }
    setStep('research-tools');
  };

  // Final completion
  const finishSetup = async () => {
    try {
      const settings = await invoke<Record<string, unknown>>('get_settings');
      await invoke('update_settings', {
        settings: { ...settings, setup_completed: true },
      });
    } catch {
      // Non-fatal
    }
    setStep('complete');
    setTimeout(onComplete, 600);
  };

  // Derive step failure from actual dependency state — not just a flag
  const isStepFailed = (key: Step): boolean => {
    if (failedSteps.has(key)) return true;
    if (!deps) return false;
    // Tools step: failed if Git (Windows) or Node is missing
    if (key === 'install-tools') {
      return (isWindows && !deps.git_bash) || !deps.node;
    }
    // Claude step: failed if Claude Code is not installed
    if (key === 'install-claude') {
      return !deps.claude_code;
    }
    // Xcode step (macOS): failed if xcode_cli missing
    if (key === 'install-xcode') {
      return !deps.xcode_cli;
    }
    return false;
  };

  // Stepper bar — compact pill layout that fits within max-w-2xl
  const StepIndicator = () => (
    <div className="flex items-center justify-center gap-0.5 mb-6 flex-wrap">
      {allSteps.map((s, i) => {
        const isPast = i < currentStepIndex;
        const isCurrent = i === currentStepIndex;
        const hasFailed = isPast && isStepFailed(s.key);

        return (
          <div key={s.key} className="flex items-center gap-0.5">
            <div
              className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors whitespace-nowrap ${
                hasFailed
                  ? 'bg-red-900/30 text-red-600 dark:text-red-400'
                  : isPast
                  ? 'bg-green-900/30 text-green-600 dark:text-green-400'
                  : isCurrent
                  ? 'bg-blue-900/40 text-blue-600 dark:text-blue-400 ring-1 ring-blue-500/30'
                  : 'bg-surface/50 text-subtle'
              }`}
            >
              {hasFailed ? (
                <XCircle className="w-3 h-3 shrink-0" />
              ) : isPast ? (
                <CheckCircle className="w-3 h-3 shrink-0" />
              ) : (
                <span className="w-3 h-3 flex items-center justify-center text-[9px] shrink-0">{i + 1}</span>
              )}
              {s.label}
            </div>
            {i < allSteps.length - 1 && (
              <div className={`w-3 h-px shrink-0 ${
                hasFailed ? 'bg-red-800' : isPast ? 'bg-green-800' : 'bg-surface'
              }`} />
            )}
          </div>
        );
      })}
    </div>
  );

  // Install step indicator component
  const InstallStepRow = ({ stepKey, label, icon: Icon }: { stepKey: string; label: string; icon: React.ElementType }) => {
    const info = installSteps[stepKey];
    const status = info?.status;

    return (
      <div className={`flex items-center gap-3 p-3 rounded-lg transition-all ${
        status === 'complete' || status === 'skipped'
          ? 'bg-green-950/10 border border-green-900/20'
          : status === 'error'
          ? 'bg-red-950/10 border border-red-900/20'
          : status && status !== 'complete' && status !== 'skipped' && status !== 'error'
          ? 'bg-blue-950/10 border border-blue-800/30'
          : 'bg-panel/50 border border-border-default/30'
      }`}>
        <div className={`w-8 h-8 rounded-full flex items-center justify-center shrink-0 ${
          status === 'complete' || status === 'skipped' ? 'bg-green-900/30' :
          status === 'error' ? 'bg-red-900/30' :
          status ? 'bg-blue-900/30' : 'bg-surface/50'
        }`}>
          {status === 'complete' || status === 'skipped' ? (
            <CheckCircle className="w-4 h-4 text-green-600 dark:text-green-400" />
          ) : status === 'error' ? (
            <AlertTriangle className="w-4 h-4 text-red-600 dark:text-red-400" />
          ) : status ? (
            <Loader2 className="w-4 h-4 text-blue-600 dark:text-blue-400 animate-spin" />
          ) : (
            <Icon className="w-4 h-4 text-subtle" />
          )}
        </div>
        <div className="flex-1 min-w-0">
          <p className={`text-sm font-medium ${
            status === 'complete' || status === 'skipped' ? 'text-green-700 dark:text-green-300' :
            status === 'error' ? 'text-red-700 dark:text-red-300' :
            status ? 'text-blue-700 dark:text-blue-300' : 'text-muted'
          }`}>
            {label}
          </p>
          {info?.message && (
            <p className="text-[10px] text-muted truncate mt-0.5">
              {info.message}
            </p>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className={mode === 'modal'
      ? 'fixed inset-0 z-[60] flex items-center justify-center'
      : 'h-screen w-screen bg-canvas flex items-center justify-center'
    }>
      {mode === 'modal' && (
        <div className="absolute inset-0 bg-black/60" onClick={onComplete} />
      )}
      <div className={`w-full max-w-2xl mx-auto px-4 py-6 sm:p-8 max-h-[90vh] overflow-y-auto overflow-x-hidden ${mode === 'modal' ? 'relative bg-panel rounded-xl border border-border-strong shadow-2xl' : ''}`}>
        {mode === 'modal' && (
          <button
            onClick={onComplete}
            className="absolute top-3 right-3 p-1 rounded hover:bg-hover text-muted hover:text-secondary"
          >
            <XCircle className="w-5 h-5" />
          </button>
        )}

        {/* Step indicator */}
        {step !== 'welcome' && step !== 'checking' && step !== 'complete' && step !== 'dependencies' && <StepIndicator />}

        {/* ========== WELCOME ========== */}
        {step === 'welcome' && (
          <div className="text-center space-y-6">
            <div className="flex justify-center">
              <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-blue-500 to-purple-600 flex items-center justify-center">
                <Sparkles className="w-8 h-8 text-white" />
              </div>
            </div>
            <div>
              <h1 className="text-2xl font-bold text-primary">Welcome to Operon</h1>
              <p className="text-secondary mt-2 text-sm leading-relaxed">
                A desktop IDE for bioinformatics — run Claude AI agents on your local machine
                and HPC compute nodes. We'll get you set up in a few steps.
              </p>
            </div>
            <div className="flex items-center justify-center gap-6 text-[11px] text-muted">
              <span className="flex items-center gap-1.5"><Code2 className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" /> Code editor</span>
              <span className="flex items-center gap-1.5"><Terminal className="w-3.5 h-3.5 text-green-600 dark:text-green-400" /> Terminal</span>
              <span className="flex items-center gap-1.5"><Bot className="w-3.5 h-3.5 text-purple-600 dark:text-purple-400" /> AI agent</span>
              <span className="flex items-center gap-1.5"><Server className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" /> SSH/HPC</span>
            </div>
            <button
              onClick={checkDeps}
              className="inline-flex items-center gap-2 px-6 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors"
            >
              Get Started
              <ArrowRight className="w-4 h-4" />
            </button>
          </div>
        )}

        {/* ========== CHECKING ========== */}
        {step === 'checking' && (
          <div className="text-center space-y-4">
            <Loader2 className="w-8 h-8 text-blue-600 dark:text-blue-400 animate-spin mx-auto" />
            <p className="text-secondary">Checking your system...</p>
          </div>
        )}

        {/* ========== PAGE 1: XCODE (macOS only) ========== */}
        {isMac && step === 'install-xcode' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-blue-900/30 flex items-center justify-center mx-auto mb-3">
                <Terminal className="w-6 h-6 text-blue-600 dark:text-blue-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Xcode Command Line Tools</h2>
              <p className="text-muted text-sm mt-1">
                Required for compiling software on macOS. This can take 20–30 minutes on slower connections.
              </p>
            </div>

            {/* Status indicator */}
            {phaseRunning && (
              <div className="space-y-2">
                <div className="flex items-center gap-3 p-3 bg-blue-950/10 border border-blue-800/30 rounded-lg">
                  <Loader2 className="w-5 h-5 text-blue-600 dark:text-blue-400 animate-spin shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium text-blue-700 dark:text-blue-300">Installing Xcode CLI tools...</p>
                    <p className="text-[10px] text-muted mt-0.5">{installMessage}</p>
                  </div>
                </div>
                <div className="h-1.5 bg-surface rounded-full overflow-hidden">
                  <div className="h-full bg-blue-500 rounded-full transition-all duration-700" style={{ width: `${installPercent}%` }} />
                </div>
                <div className="p-2.5 bg-amber-950/20 border border-amber-800/20 rounded-lg">
                  <p className="text-[10px] text-amber-700 dark:text-amber-300/80 leading-relaxed">
                    A macOS dialog will appear asking you to install. Click "Install" and wait for it to finish.
                    This window will update automatically.
                  </p>
                </div>
              </div>
            )}

            {phaseDone && !phaseError && (
              <div className="flex items-center gap-3 p-3 bg-green-950/10 border border-green-900/20 rounded-lg">
                <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                <p className="text-sm font-medium text-green-700 dark:text-green-300">Xcode CLI tools installed!</p>
              </div>
            )}

            {phaseError && (
              <div className="flex items-start gap-3 p-3 bg-red-950/10 border border-red-900/20 rounded-lg">
                <AlertTriangle className="w-5 h-5 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm font-medium text-red-700 dark:text-red-300">Auto-install failed</p>
                  <p className="text-[10px] text-muted mt-0.5">{phaseError}</p>
                </div>
              </div>
            )}

            {/* Always show fallback command */}
            {phaseDone && (
              <div className="p-3 bg-panel border border-border-strong rounded-lg space-y-2">
                <p className="text-[11px] text-secondary font-medium">
                  {phaseError ? 'Run this in Terminal instead:' : 'Or install manually via Terminal:'}
                </p>
                <code className="block text-[11px] text-green-700 dark:text-green-300 bg-canvas px-3 py-2 rounded font-mono select-all">
                  xcode-select --install
                </code>
                <p className="text-[9px] text-subtle">Click the command to select it, then paste into Terminal.app.</p>
              </div>
            )}

            {/* Action buttons */}
            <div className="flex gap-3">
              {!phaseRunning && !phaseDone && (
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); startXcodeInstall(); }}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors"
                >
                  <Download className="w-4 h-4" />
                  Install Xcode CLI Tools
                </button>
              )}
              {phaseDone && !phaseError && (
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-tools'); }}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"
                >
                  Continue
                  <ArrowRight className="w-4 h-4" />
                </button>
              )}
              {phaseError && (
                <>
                  <button
                    onClick={() => { setPhaseDone(false); setPhaseError(null); startXcodeInstall(); }}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors"
                  >
                    <Download className="w-4 h-4" />
                    Retry
                  </button>
                  <button
                    onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-tools'); }}
                    className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"
                  >
                    I installed it manually →
                  </button>
                </>
              )}
            </div>

            {!phaseRunning && !phaseDone && (
              <div className="text-center">
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-tools'); }}
                  className="text-xs text-subtle hover:text-secondary transition-colors"
                >
                  Already installed? Skip →
                </button>
              </div>
            )}
          </div>
        )}

        {/* ========== PAGE 2: HOMEBREW + NODE + GH ========== */}
        {step === 'install-tools' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-amber-900/30 flex items-center justify-center mx-auto mb-3">
                <Package className="w-6 h-6 text-amber-600 dark:text-amber-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Developer Tools</h2>
              <p className="text-muted text-sm mt-1">
                {isMac
                  ? 'Installing Homebrew, Node.js, and GitHub CLI. This usually takes a couple of minutes.'
                  : isWindows
                  ? 'Installing Git, Node.js, and other developer tools.'
                  : 'Installing Node.js and GitHub CLI. This usually takes a couple of minutes.'}
              </p>
            </div>

            {/* Windows: OpenSSH client availability — needed for remote connections */}
            {isWindows && sshStatus && (
              <div className="p-3 bg-panel border border-border-strong rounded-lg">
                <div className="flex items-start gap-2.5 min-w-0">
                  {sshStatus.available ? (
                    <CheckCircle className="w-4 h-4 text-green-600 dark:text-green-400 shrink-0 mt-0.5" />
                  ) : (
                    <XCircle className="w-4 h-4 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                  )}
                  <div className="min-w-0 space-y-1.5">
                    <p className="text-xs font-medium text-secondary">
                      OpenSSH Client {sshStatus.available ? 'detected' : 'not found'}
                    </p>
                    {!sshStatus.available && (
                      <>
                        <p className="text-[11px] text-muted leading-relaxed">
                          OpenSSH client not found. Enable it via Settings &gt; Apps &gt; Optional
                          Features &gt; Add &apos;OpenSSH Client&apos;, or run in PowerShell (admin):
                        </p>
                        <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                          Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0
                        </code>
                      </>
                    )}
                  </div>
                </div>
              </div>
            )}

            {/* Windows: Git is installed by the unified elevated batch (install_phase_tools,
                step [1/6]) along with Node/gh/Python/uv/OpenSSH in a single UAC prompt.
                We intentionally do NOT show a separate "Download Git" pre-step here — that
                made the user install Git by hand and then the batch re-installed it
                (double-install). If the batch can't produce Git, the phaseError panel below
                ("Action Required: Install Git for Windows") is the single fallback. */}

            {/* Per-step status rows */}
            {(phaseRunning || phaseDone) && (
              <div className="space-y-1.5">
                {isWindows && <InstallStepRow stepKey="git" label="Git for Windows (required by Claude Code)" icon={GitBranch} />}
                {isMac && <InstallStepRow stepKey="homebrew" label="Homebrew Package Manager" icon={Download} />}
                <InstallStepRow stepKey="node" label="Node.js Runtime" icon={Package} />
                <InstallStepRow stepKey="gh" label="GitHub CLI" icon={GitBranch} />
                {isWindows && <InstallStepRow stepKey="python" label="Python (for PDF reports & research tools)" icon={Terminal} />}
                {isWindows && <InstallStepRow stepKey="openssh" label="OpenSSH Client (for remote connections)" icon={Globe} />}
                {isWindows && <InstallStepRow stepKey="uv" label="uv Package Manager (for research tools)" icon={Zap} />}
                <InstallStepRow stepKey="reportlab" label="PDF Report Library" icon={FileText} />
              </div>
            )}

            {phaseRunning && (
              <div className="space-y-1.5">
                <div className="flex justify-between text-[10px] text-muted">
                  <span>{installMessage}</span>
                  <span>{installPercent}%</span>
                </div>
                <div className="h-1.5 bg-surface rounded-full overflow-hidden">
                  <div className="h-full bg-amber-500 rounded-full transition-all duration-700" style={{ width: `${installPercent}%` }} />
                </div>
                {isMac && (
                  <div className="p-2.5 bg-amber-950/20 border border-amber-800/20 rounded-lg">
                    <p className="text-[10px] text-amber-700 dark:text-amber-300/80 leading-relaxed">
                      macOS may ask for your password once to create the Homebrew directory. This is normal.
                    </p>
                  </div>
                )}
              </div>
            )}

            {phaseDone && !phaseError && (
              <div className="flex items-center gap-3 p-3 bg-green-950/10 border border-green-900/20 rounded-lg">
                <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                <p className="text-sm font-medium text-green-700 dark:text-green-300">All tools installed!</p>
              </div>
            )}

            {phaseError && installSteps['git']?.status === 'error' && (
              <div className="space-y-3">
                <div className="flex items-start gap-3 p-4 bg-amber-950/20 border-2 border-amber-700/40 rounded-lg">
                  <AlertTriangle className="w-6 h-6 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                  <div>
                    <p className="text-sm font-semibold text-amber-200">Action Required: Install Git for Windows</p>
                    <p className="text-xs text-secondary mt-2 leading-relaxed">
                      {installSteps['git']?.message?.includes('installer launched')
                        ? 'The Git installer should have appeared on your screen. Complete the setup wizard (just click Next → Next → Install) and wait for it to finish.'
                        : 'A download page should have opened in your browser. Download and run the Git installer — accept the defaults and click Next through the wizard.'}
                    </p>
                    <div className="mt-3 p-2 bg-panel/80 rounded border border-border-strong">
                      <p className="text-[10px] text-secondary font-medium mb-1">If you don't see the installer:</p>
                      <p className="text-[10px] text-muted">Open PowerShell and run:</p>
                      <code className="block text-[11px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono select-all mt-1">
                        winget install Git.Git
                      </code>
                    </div>
                  </div>
                </div>
                <button
                  onClick={async () => {
                    try {
                      // Refresh PATH on the backend first
                      await invoke('refresh_environment').catch(() => {});
                      const status = await invoke<DependencyStatus>('check_local_dependencies');
                      setDeps(status);
                      if (status.git_bash) {
                        setPhaseError(null);
                        setFailedSteps(prev => { const next = new Set(prev); next.delete('install-tools' as Step); return next; });
                        // Re-run tools phase to install remaining tools now that Git is present
                        setPhaseDone(false);
                        startToolsInstall();
                      } else {
                        setPhaseError('Git Bash still not detected. Make sure the Git installer finished completely, then try again.');
                      }
                    } catch { /* ignore */ }
                  }}
                  className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-amber-600 hover:bg-amber-500 text-white rounded-lg font-semibold text-base transition-colors"
                >
                  <CheckCircle className="w-5 h-5" />
                  I finished installing Git — Re-check
                </button>
              </div>
            )}

            {phaseError && installSteps['git']?.status !== 'error' && (
              <div className="flex items-start gap-3 p-3 bg-red-950/10 border border-red-900/20 rounded-lg">
                <AlertTriangle className="w-5 h-5 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm font-medium text-red-700 dark:text-red-300">Some tools failed to install</p>
                  <p className="text-[10px] text-muted mt-0.5">{phaseError}</p>
                </div>
              </div>
            )}

            {/* Always show fallback terminal commands when done (whether success or failure) */}
            {phaseDone && (
              <div className="p-3 bg-panel border border-border-strong rounded-lg space-y-2">
                <p className="text-[11px] text-secondary font-medium">
                  {phaseError ? 'Run these in Terminal instead:' : 'Or install manually via Terminal:'}
                </p>
                <div className="space-y-1.5">
                  {isWindows && (
                    <div>
                      <p className="text-[9px] text-muted mb-0.5">Git for Windows (required by Claude Code):</p>
                      <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                        winget install -e --id Git.Git --source winget --accept-source-agreements --accept-package-agreements
                      </code>
                    </div>
                  )}
                  {isMac && (
                    <div>
                      <p className="text-[9px] text-muted mb-0.5">Homebrew:</p>
                      <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                        /bin/bash -c &quot;$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)&quot;
                      </code>
                    </div>
                  )}
                  <div>
                    <p className="text-[9px] text-muted mb-0.5">Node.js{!isMac && ' & GitHub CLI'}:</p>
                    <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                      {isMac
                        ? 'brew install node gh'
                        : isWindows
                        ? 'winget install -e --id OpenJS.NodeJS.LTS --source winget --accept-source-agreements --accept-package-agreements; winget install -e --id GitHub.cli --source winget --accept-source-agreements --accept-package-agreements'
                        : 'sudo apt install -y nodejs gh'}
                    </code>
                  </div>
                  {isMac && (
                    <div>
                      <p className="text-[9px] text-muted mb-0.5">GitHub CLI:</p>
                      <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                        (included in brew install above)
                      </code>
                    </div>
                  )}
                  {isWindows && (
                    <>
                      <div>
                        <p className="text-[9px] text-muted mb-0.5">Python:</p>
                        <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                          winget install -e --id Python.Python.3.12 --source winget --accept-source-agreements --accept-package-agreements
                        </code>
                      </div>
                      <div>
                        <p className="text-[9px] text-muted mb-0.5">OpenSSH Client (usually pre-installed; run in admin PowerShell):</p>
                        <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                          Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0
                        </code>
                      </div>
                      <div>
                        <p className="text-[9px] text-muted mb-0.5">uv (Python package manager):</p>
                        <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                          winget install -e --id astral-sh.uv --source winget --accept-source-agreements --accept-package-agreements
                        </code>
                      </div>
                    </>
                  )}
                  <div>
                    <p className="text-[9px] text-muted mb-0.5">PDF Report Library:</p>
                    <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                      {isWindows ? 'pip install reportlab' : 'pip3 install reportlab'}
                    </code>
                  </div>
                </div>
                <p className="text-[9px] text-subtle">
                  Click a command to select it, paste into {isMac ? 'Terminal.app' : isWindows ? 'PowerShell' : 'your terminal'}. Hit Retry after.
                </p>
              </div>
            )}

            {/* Action buttons */}
            <div className="flex gap-3">
              {!phaseRunning && !phaseDone && (
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); startToolsInstall(); }}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-amber-600 hover:bg-amber-500 text-white rounded-lg font-medium transition-colors"
                >
                  <Download className="w-4 h-4" />
                  Install Tools
                </button>
              )}
              {phaseDone && !phaseError && (
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-claude'); }}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"
                >
                  Continue
                  <ArrowRight className="w-4 h-4" />
                </button>
              )}
              {phaseError && (
                <>
                  <button
                    onClick={() => { setPhaseDone(false); setPhaseError(null); startToolsInstall(); }}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-amber-600 hover:bg-amber-500 text-white rounded-lg font-medium transition-colors"
                  >
                    <Download className="w-4 h-4" />
                    Retry
                  </button>
                  <button
                    onClick={async () => {
                      // Re-check deps before allowing skip — block if critical tools missing
                      await invoke('refresh_environment').catch(() => {});
                      const status = await invoke<DependencyStatus>('check_local_dependencies');
                      setDeps(status);
                      const gitOk = !isWindows || status.git_bash;
                      if (gitOk && status.node) {
                        setPhaseDone(false); setPhaseError(null); setStep('install-claude');
                      } else {
                        setPhaseError(
                          !gitOk
                            ? 'Git Bash is still not installed. Please install Git for Windows before continuing.'
                            : 'Node.js is still not installed. Please install it before continuing.'
                        );
                      }
                    }}
                    className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"
                  >
                    I installed manually →
                  </button>
                </>
              )}
            </div>

            {!phaseRunning && !phaseDone && (
              <div className="flex items-center justify-center gap-4">
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep(isMac ? 'install-xcode' : 'welcome'); }}
                  className="text-xs text-subtle hover:text-secondary transition-colors"
                >
                  ← Back
                </button>
                <button
                  onClick={async () => {
                    // Re-check deps before allowing skip — block if critical tools missing
                    await invoke('refresh_environment').catch(() => {});
                    const status = await invoke<DependencyStatus>('check_local_dependencies');
                    setDeps(status);
                    const gitOk = !isWindows || status.git_bash;
                    if (gitOk && status.node) {
                      setPhaseDone(false); setPhaseError(null); setStep('install-claude');
                    } else {
                      // Can't skip — show the install UI with error
                      setPhaseDone(true);
                      setPhaseError(
                        !gitOk
                          ? 'Git for Windows is required. Install it first, then click Retry.'
                          : 'Node.js is required. Install it first, then click Retry.'
                      );
                      // Populate git step status so the amber panel shows
                      if (!gitOk) {
                        setInstallSteps(prev => ({ ...prev, git: { status: 'error', message: 'Git not found' } }));
                      }
                    }
                  }}
                  className="text-xs text-subtle hover:text-secondary transition-colors"
                >
                  Already installed? Skip →
                </button>
              </div>
            )}
          </div>
        )}

        {/* ========== PAGE 3: CLAUDE CODE ========== */}
        {step === 'install-claude' && (() => {
          const gitBashMissing = isWindows && deps && !deps.git_bash;
          const nodeMissing = deps && !deps.node;
          const hasPrereqIssue = gitBashMissing || nodeMissing;

          return (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-purple-900/30 flex items-center justify-center mx-auto mb-3">
                <Bot className="w-6 h-6 text-purple-600 dark:text-purple-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Claude Code</h2>
              <p className="text-muted text-sm mt-1">
                The AI coding agent that powers Operon.
              </p>
            </div>

            {/* Prerequisite missing — redirect back to Tools step */}
            {hasPrereqIssue && !phaseRunning && (
              <div className="space-y-4">
                <div className="flex items-start gap-3 p-4 bg-red-950/20 border-2 border-red-700/40 rounded-lg">
                  <XCircle className="w-6 h-6 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                  <div>
                    <p className="text-sm font-semibold text-red-200">
                      {gitBashMissing ? 'Git for Windows must be installed first' : 'Node.js must be installed first'}
                    </p>
                    <p className="text-xs text-secondary mt-1.5">
                      {gitBashMissing
                        ? 'Claude Code requires Git Bash to run on Windows. Go back to the Tools step to install it.'
                        : 'Claude Code requires Node.js. Go back to the Tools step to install it.'}
                    </p>
                  </div>
                </div>
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-tools'); }}
                  className="w-full flex items-center justify-center gap-2 px-4 py-3 bg-amber-600 hover:bg-amber-500 text-white rounded-lg font-semibold text-base transition-colors"
                >
                  <ArrowLeft className="w-5 h-5" />
                  Go Back to Install Tools
                </button>
              </div>
            )}

            {phaseRunning && (
              <div className="space-y-2">
                <div className="flex items-center gap-3 p-3 bg-purple-950/10 border border-purple-800/30 rounded-lg">
                  <Loader2 className="w-5 h-5 text-purple-600 dark:text-purple-400 animate-spin shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium text-purple-700 dark:text-purple-300">Installing Claude Code...</p>
                    <p className="text-[10px] text-muted mt-0.5">{installMessage}</p>
                  </div>
                </div>
                <div className="h-1.5 bg-surface rounded-full overflow-hidden">
                  <div className="h-full bg-purple-500 rounded-full transition-all duration-700" style={{ width: `${installPercent}%` }} />
                </div>
              </div>
            )}

            {phaseDone && !phaseError && (
              <div className="flex items-center gap-3 p-3 bg-green-950/10 border border-green-900/20 rounded-lg">
                <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                <p className="text-sm font-medium text-green-700 dark:text-green-300">Claude Code installed!</p>
              </div>
            )}

            {phaseError && (
              <div className="flex items-start gap-3 p-3 bg-red-950/10 border border-red-900/20 rounded-lg">
                <AlertTriangle className="w-5 h-5 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                <div>
                  <p className="text-sm font-medium text-red-700 dark:text-red-300">Installation failed</p>
                  <p className="text-[10px] text-muted mt-0.5">{phaseError}</p>
                </div>
              </div>
            )}

            {/* Always show fallback command */}
            {phaseDone && (
              <div className="p-3 bg-panel border border-border-strong rounded-lg space-y-2">
                <p className="text-[11px] text-secondary font-medium">
                  {phaseError ? 'Run this in Terminal instead:' : 'Or install manually via Terminal:'}
                </p>
                {isWindows && (
                  <div className="mb-1.5">
                    <p className="text-[9px] text-muted mb-0.5">1. Install Git Bash (if not done):</p>
                    <code className="block text-[10px] text-green-700 dark:text-green-300 bg-canvas px-2 py-1.5 rounded font-mono break-all overflow-x-auto select-all">
                      winget install Git.Git
                    </code>
                  </div>
                )}
                <div>
                  <p className="text-[9px] text-muted mb-0.5">{isWindows ? '2. ' : ''}Install Claude Code:</p>
                  <code className="block text-[11px] text-green-700 dark:text-green-300 bg-canvas px-3 py-2 rounded font-mono select-all">
                    {isWindows
                      ? 'npm install -g @anthropic-ai/claude-code'
                      : 'curl -fsSL https://claude.ai/install.sh | bash'}
                  </code>
                </div>
                <p className="text-[9px] text-subtle">Click a command to select it, then paste into {isMac ? 'Terminal.app' : isWindows ? 'PowerShell' : 'your terminal'}.</p>
              </div>
            )}

            {/* Action buttons */}
            <div className="flex gap-3">
              {!phaseRunning && !phaseDone && !hasPrereqIssue && (
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); startClaudeInstall(); }}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-purple-600 hover:bg-purple-500 text-white rounded-lg font-medium transition-colors"
                >
                  <Download className="w-4 h-4" />
                  Install Claude Code
                </button>
              )}
              {phaseDone && !phaseError && (
                <button
                  onClick={() => setStep('auth')}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"
                >
                  Continue to Authentication
                  <ArrowRight className="w-4 h-4" />
                </button>
              )}
              {phaseError && (
                <>
                  <button
                    onClick={() => { setPhaseDone(false); setPhaseError(null); startClaudeInstall(); }}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-purple-600 hover:bg-purple-500 text-white rounded-lg font-medium transition-colors"
                  >
                    <Download className="w-4 h-4" />
                    Retry
                  </button>
                  <button
                    onClick={() => setStep('auth')}
                    className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"
                  >
                    Skip
                  </button>
                </>
              )}
            </div>

            {!phaseRunning && !phaseDone && (
              <div className="flex items-center justify-center gap-4">
                <button
                  onClick={() => { setPhaseDone(false); setPhaseError(null); setStep('install-tools'); }}
                  className="text-xs text-subtle hover:text-secondary transition-colors"
                >
                  ← Back
                </button>
                <button
                  onClick={() => setStep('auth')}
                  className="text-xs text-subtle hover:text-secondary transition-colors"
                >
                  Already installed? Skip →
                </button>
              </div>
            )}
          </div>
          );
        })()}

        {/* ========== INSTALLING (Legacy — kept for backward compat) ========== */}
        {step === 'installing' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-blue-900/30 flex items-center justify-center mx-auto mb-3">
                {installDone && !installHadErrors
                  ? <CheckCircle className="w-6 h-6 text-green-600 dark:text-green-400" />
                  : <Loader2 className="w-6 h-6 text-blue-600 dark:text-blue-400 animate-spin" />
                }
              </div>
              <h2 className="text-lg font-semibold text-primary">
                {installDone && !installHadErrors
                  ? 'All set!'
                  : installDone && installHadErrors
                  ? 'Almost there'
                  : 'Setting up your system'}
              </h2>
              <p className="text-muted text-sm mt-1">
                {installDone && !installHadErrors
                  ? 'Everything is installed and ready to go.'
                  : installDone && installHadErrors
                  ? 'Some items may need manual attention.'
                  : 'Installing dependencies automatically — this only happens once.'}
              </p>
            </div>
            <div className="space-y-1.5">
              <div className="flex justify-between text-[10px] text-muted">
                <span>{installMessage}</span>
                <span>{installPercent}%</span>
              </div>
              <div className="h-2 bg-surface rounded-full overflow-hidden">
                <div
                  className={`h-full rounded-full transition-all duration-700 ease-out ${
                    installDone && !installHadErrors ? 'bg-green-500' :
                    installHadErrors ? 'bg-amber-500' : 'bg-blue-500'
                  }`}
                  style={{ width: `${installPercent}%` }}
                />
              </div>
            </div>
            <div className="space-y-1.5">
              {isMac && <InstallStepRow stepKey="xcode" label="Xcode Command Line Tools" icon={Terminal} />}
              {isMac && <InstallStepRow stepKey="homebrew" label="Homebrew Package Manager" icon={Download} />}
              <InstallStepRow stepKey="node" label="Node.js Runtime" icon={Package} />
              <InstallStepRow stepKey="claude" label="Claude Code" icon={Bot} />
            </div>
            {installDone && (
              <div className="flex gap-3">
                <button
                  onClick={() => setStep('auth')}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"
                >
                  Continue
                  <ArrowRight className="w-4 h-4" />
                </button>
              </div>
            )}
          </div>
        )}

        {/* ========== DEPENDENCIES (Manual fallback — shown on error) ========== */}
        {step === 'dependencies' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-red-900/30 flex items-center justify-center mx-auto mb-3">
                <AlertTriangle className="w-6 h-6 text-red-600 dark:text-red-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">System Check</h2>
              <p className="text-muted text-sm mt-1">
                We couldn't automatically detect your system configuration.
              </p>
            </div>
            {error && (
              <div className="p-3 bg-red-950/20 border border-red-900/40 rounded-lg">
                <div className="flex items-start gap-2">
                  <AlertTriangle className="w-4 h-4 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                  <p className="text-xs text-red-700 dark:text-red-300">{error}</p>
                </div>
              </div>
            )}
            <div className="flex gap-3">
              <button
                onClick={startInstall}
                className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors"
              >
                <Download className="w-4 h-4" />
                Install Everything
              </button>
              <button
                onClick={() => setStep('auth')}
                className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"
              >
                Skip
              </button>
            </div>
          </div>
        )}

        {/* ========== AUTHENTICATION ========== */}
        {step === 'auth' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-blue-900/30 flex items-center justify-center mx-auto mb-3">
                <Key className="w-6 h-6 text-blue-600 dark:text-blue-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Authentication</h2>
              <p className="text-muted text-sm mt-1">
                Choose how to authenticate with Claude. You can change this later in settings.
              </p>
            </div>

            {/* Auth method selector */}
            {!authMethod && (
              <div className="space-y-2.5">
                <button
                  onClick={() => setAuthMethod('api')}
                  className="w-full flex items-start gap-3.5 p-4 bg-panel rounded-lg border border-border-default hover:border-blue-700/50 hover:bg-panel/80 transition-colors text-left group"
                >
                  <div className="w-9 h-9 rounded-lg bg-amber-900/30 flex items-center justify-center shrink-0 mt-0.5">
                    <Key className="w-4 h-4 text-amber-600 dark:text-amber-400" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-primary group-hover:text-primary">Anthropic API Key</p>
                    <p className="text-xs text-muted mt-0.5 leading-relaxed">
                      Use your own API key from console.anthropic.com. Pay per token — best for heavy or automated usage.
                    </p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-subtle group-hover:text-secondary mt-2.5 ml-auto shrink-0" />
                </button>

                <button
                  onClick={() => setAuthMethod('oauth')}
                  className="w-full flex items-start gap-3.5 p-4 bg-panel rounded-lg border border-border-default hover:border-purple-700/50 hover:bg-panel/80 transition-colors text-left group"
                >
                  <div className="w-9 h-9 rounded-lg bg-purple-900/30 flex items-center justify-center shrink-0 mt-0.5">
                    <LogIn className="w-4 h-4 text-purple-600 dark:text-purple-400" />
                  </div>
                  <div>
                    <p className="text-sm font-medium text-primary group-hover:text-primary">Claude Pro / Team / Enterprise</p>
                    <p className="text-xs text-muted mt-0.5 leading-relaxed">
                      Sign in with your Claude subscription. Uses OAuth — Claude Code will open a browser window to log you in.
                    </p>
                  </div>
                  <ArrowRight className="w-4 h-4 text-subtle group-hover:text-secondary mt-2.5 ml-auto shrink-0" />
                </button>
              </div>
            )}

            {/* API Key input */}
            {authMethod === 'api' && (
              <div className="space-y-3">
                <div className="flex items-center gap-2 mb-1">
                  <Key className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" />
                  <span className="text-xs text-secondary font-medium">Anthropic API Key</span>
                  <button
                    onClick={() => setAuthMethod(null)}
                    className="text-[10px] text-subtle hover:text-secondary ml-auto"
                  >
                    Change method
                  </button>
                </div>
                <input
                  type="password"
                  value={apiKey}
                  onChange={(e) => setApiKey(e.target.value)}
                  placeholder="sk-ant-..."
                  className="w-full bg-panel border border-border-default rounded-lg px-4 py-2.5 text-sm text-primary font-mono outline-none focus:border-blue-600 placeholder:text-subtle"
                  spellCheck={false}
                  autoFocus
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && apiKey.trim()) completeAuth();
                  }}
                />
                <p className="text-xs text-subtle">
                  Get your API key from{' '}
                  <a href="https://console.anthropic.com/settings/keys" target="_blank" rel="noopener noreferrer" className="text-blue-600 dark:text-blue-400 hover:text-blue-800 dark:hover:text-blue-700">
                    console.anthropic.com
                  </a>
                </p>
              </div>
            )}

            {/* OAuth flow */}
            {authMethod === 'oauth' && (
              <div className="space-y-3">
                <div className="flex items-center gap-2 mb-1">
                  <LogIn className="w-3.5 h-3.5 text-purple-600 dark:text-purple-400" />
                  <span className="text-xs text-secondary font-medium">Claude Subscription (OAuth)</span>
                  <button
                    onClick={() => { setAuthMethod(null); setOauthState('idle'); setOauthMessage(null); setError(null); }}
                    className="text-[10px] text-subtle hover:text-secondary ml-auto"
                  >
                    Change method
                  </button>
                </div>

                {oauthState === 'idle' && (
                  <div className="p-4 bg-panel rounded-lg border border-border-default space-y-3">
                    <p className="text-sm text-secondary leading-relaxed">
                      Click below to sign in. A browser window will open for you to log into your Claude account.
                    </p>
                    <p className="text-xs text-muted leading-relaxed">
                      Works with Claude Pro ($20/mo), Team, and Enterprise subscriptions.
                    </p>
                  </div>
                )}

                {oauthState === 'launched' && (
                  <div className="p-4 bg-purple-950/20 rounded-lg border border-purple-900/30 space-y-3">
                    <div className="flex items-center gap-2">
                      <Loader2 className="w-4 h-4 text-purple-600 dark:text-purple-400 animate-spin" />
                      <p className="text-sm text-purple-700 dark:text-purple-300 font-medium">Waiting for you to sign in...</p>
                    </div>
                    <p className="text-xs text-secondary leading-relaxed">
                      A browser window should have opened. Sign in with your Claude account there.
                      This page will update automatically once you're logged in.
                    </p>
                    {oauthMessage && <p className="text-[10px] text-muted">{oauthMessage}</p>}
                  </div>
                )}

                {oauthState === 'checking' && (
                  <div className="p-4 bg-panel rounded-lg border border-border-default flex items-center gap-3">
                    <Loader2 className="w-4 h-4 text-purple-600 dark:text-purple-400 animate-spin" />
                    <p className="text-sm text-secondary">Checking authentication...</p>
                  </div>
                )}

                {oauthState === 'success' && (
                  <div className="p-4 bg-green-950/20 rounded-lg border border-green-900/30 flex items-center gap-3">
                    <CheckCircle className="w-5 h-5 text-green-600 dark:text-green-400" />
                    <div>
                      <p className="text-sm text-green-700 dark:text-green-300 font-medium">Logged in successfully!</p>
                      <p className="text-xs text-muted">Credentials verified. Continuing...</p>
                    </div>
                  </div>
                )}

                {oauthState === 'failed' && (
                  <div className="p-4 bg-amber-950/20 rounded-lg border border-amber-800/30 space-y-2">
                    <p className="text-xs text-secondary leading-relaxed">
                      Login not detected yet. Make sure you completed the sign-in in the browser window.
                    </p>
                    <p className="text-[10px] text-muted">
                      If the browser didn't open, try clicking "Relaunch Login" below.
                    </p>
                  </div>
                )}
              </div>
            )}

            {error && (
              <div className="flex items-start gap-2 p-3 bg-red-950/30 border border-red-900/50 rounded-lg">
                <AlertTriangle className="w-4 h-4 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                <p className="text-xs text-red-700 dark:text-red-300">{error}</p>
              </div>
            )}

            {/* Action buttons for API key */}
            {authMethod === 'api' && (
              <div className="flex gap-3">
                <button
                  onClick={() => completeAuth(false)}
                  disabled={!apiKey.trim()}
                  className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Save & Continue
                  <ArrowRight className="w-4 h-4" />
                </button>
              </div>
            )}

            {/* Action buttons for OAuth */}
            {authMethod === 'oauth' && oauthState !== 'success' && (
              <div className="flex gap-3">
                {(oauthState === 'idle' || oauthState === 'failed') && (
                  <button
                    onClick={launchOAuth}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-purple-600 hover:bg-purple-500 text-white rounded-lg font-medium transition-colors"
                  >
                    <LogIn className="w-4 h-4" />
                    {oauthState === 'failed' ? 'Relaunch Login' : 'Sign In with Claude'}
                  </button>
                )}
                {oauthState === 'launched' && (
                  <button
                    onClick={verifyOAuth}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-elevated hover:bg-elevated text-primary rounded-lg font-medium transition-colors"
                  >
                    <CheckCircle className="w-4 h-4" />
                    Check Now
                  </button>
                )}
                <button
                  onClick={() => {
                    if (oauthPollRef.current) clearInterval(oauthPollRef.current);
                    oauthPollRef.current = null;
                    completeAuth(true);
                  }}
                  className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"
                >
                  Skip
                </button>
              </div>
            )}

            {!authMethod && (
              <button
                onClick={() => completeAuth(true)}
                className="w-full text-center text-xs text-subtle hover:text-secondary transition-colors py-1"
              >
                Skip authentication for now
              </button>
            )}
          </div>
        )}

        {/* ========== RESEARCH TOOLS (MCP) ========== */}
        {step === 'research-tools' && (
          <ResearchToolsStep onContinue={() => setStep('tour-overview')} />
        )}

        {/* ========== TOUR: OVERVIEW ========== */}
        {step === 'tour-overview' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-blue-900/30 flex items-center justify-center mx-auto mb-3">
                <MonitorSmartphone className="w-6 h-6 text-blue-600 dark:text-blue-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Your Workspace</h2>
              <p className="text-muted text-sm mt-1">Here's what you'll find in Operon.</p>
            </div>
            <div className="grid grid-cols-2 gap-2.5">
              <div className="p-3 bg-panel rounded-lg border border-border-default">
                <FolderTree className="w-4 h-4 text-blue-600 dark:text-blue-400 mb-2" />
                <p className="text-xs font-medium text-primary">File Explorer</p>
                <p className="text-[10px] text-muted mt-0.5 leading-relaxed">Browse your project tree in the left sidebar. Click any file to open it in the editor.</p>
              </div>
              <div className="p-3 bg-panel rounded-lg border border-border-default">
                <Code2 className="w-4 h-4 text-green-600 dark:text-green-400 mb-2" />
                <p className="text-xs font-medium text-primary">Code Editor</p>
                <p className="text-[10px] text-muted mt-0.5 leading-relaxed">Full Monaco editor with syntax highlighting, diff view, and multi-tab support.</p>
              </div>
              <div className="p-3 bg-panel rounded-lg border border-border-default">
                <Terminal className="w-4 h-4 text-amber-600 dark:text-amber-400 mb-2" />
                <p className="text-xs font-medium text-primary">Integrated Terminal</p>
                <p className="text-[10px] text-muted mt-0.5 leading-relaxed">Run shell commands directly. The terminal lives in the bottom panel.</p>
              </div>
              <div className="p-3 bg-panel rounded-lg border border-border-default">
                <MessageSquare className="w-4 h-4 text-purple-600 dark:text-purple-400 mb-2" />
                <p className="text-xs font-medium text-primary">AI Chat</p>
                <p className="text-[10px] text-muted mt-0.5 leading-relaxed">Chat with Claude in the right panel. It can read, edit, and run code in your project.</p>
              </div>
            </div>
            <div className="flex gap-3">
              <button onClick={() => setStep('tour-modes')} className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">
                Next: AI Modes <ArrowRight className="w-4 h-4" />
              </button>
              <button onClick={finishSetup} className="px-4 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm">Skip tour</button>
            </div>
          </div>
        )}

        {/* ========== TOUR: AI MODES ========== */}
        {step === 'tour-modes' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-purple-900/30 flex items-center justify-center mx-auto mb-3">
                <Bot className="w-6 h-6 text-purple-600 dark:text-purple-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Four Ways to Work with Claude</h2>
              <p className="text-muted text-sm mt-1">Switch between modes using the selector above the chat input.</p>
            </div>
            <div className="space-y-2.5">
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-blue-900/30 flex items-center justify-center shrink-0"><Bot className="w-4 h-4 text-blue-600 dark:text-blue-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Agent Mode</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Claude reads files, writes code, runs commands, and makes changes autonomously. Best for: implementing features, fixing bugs, running pipelines.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-amber-900/30 flex items-center justify-center shrink-0"><ClipboardList className="w-4 h-4 text-amber-600 dark:text-amber-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Plan Mode</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Claude creates an implementation plan before writing code. Review, give feedback, then approve. Best for: complex tasks where you want control.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-green-900/30 flex items-center justify-center shrink-0"><MessageSquare className="w-4 h-4 text-green-600 dark:text-green-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Ask Mode</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Claude answers questions with optional PubMed-grounded citations. No file changes. Best for: understanding code, scientific Q&A.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-purple-900/30 flex items-center justify-center shrink-0"><FlaskConical className="w-4 h-4 text-purple-600 dark:text-purple-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Report Mode</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Claude generates a publication-ready PDF report from your analysis files with PubMed citations and formatted figures. Best for: summarizing results, drafting manuscripts.</p>
                </div>
              </div>
            </div>
            <div className="flex gap-3">
              <button onClick={() => setStep('tour-overview')} className="flex items-center gap-1.5 px-3 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"><ArrowLeft className="w-3.5 h-3.5" /> Back</button>
              <button onClick={() => setStep('tour-remote')} className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">Next: Remote SSH <ArrowRight className="w-4 h-4" /></button>
            </div>
          </div>
        )}

        {/* ========== TOUR: REMOTE SSH ========== */}
        {step === 'tour-remote' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-teal-900/30 flex items-center justify-center mx-auto mb-3">
                <Server className="w-6 h-6 text-teal-600 dark:text-teal-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Run on HPC & Remote Servers</h2>
              <p className="text-muted text-sm mt-1">Operon can run Claude agents on remote compute nodes via SSH.</p>
            </div>
            <div className="space-y-3">
              <div className="p-4 bg-panel rounded-lg border border-border-default space-y-3">
                <div className="flex items-center gap-2"><Server className="w-4 h-4 text-teal-600 dark:text-teal-400" /><span className="text-sm font-medium text-primary">How it works</span></div>
                <div className="space-y-2 text-xs text-secondary leading-relaxed">
                  <p><span className="text-secondary font-medium">1.</span> Add your SSH server in the sidebar (host, username, key).</p>
                  <p><span className="text-secondary font-medium">2.</span> Connect — Operon auto-detects SLURM accounts, partitions, conda envs and saves them to your profile.</p>
                  <p><span className="text-secondary font-medium">3.</span> Claude runs inside a tmux session on the remote machine. Your server config is injected into every generated script.</p>
                </div>
              </div>
              <div className="p-3 bg-teal-950/20 rounded-lg border border-teal-900/30">
                <p className="text-xs text-teal-700 dark:text-teal-300/80 leading-relaxed">
                  <span className="font-medium">For HPC users:</span> Supports Duo MFA, SSH key setup, and persistent sessions. Authenticate once — Operon keeps the connection alive across app restarts.
                </p>
              </div>
            </div>
            <div className="flex gap-3">
              <button onClick={() => setStep('tour-modes')} className="flex items-center gap-1.5 px-3 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"><ArrowLeft className="w-3.5 h-3.5" /> Back</button>
              <button onClick={() => setStep('tour-features')} className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">Next: Key Features <ArrowRight className="w-4 h-4" /></button>
            </div>
          </div>
        )}

        {/* ========== TOUR: KEY FEATURES ========== */}
        {step === 'tour-features' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-green-900/30 flex items-center justify-center mx-auto mb-3">
                <FlaskConical className="w-6 h-6 text-green-600 dark:text-green-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Built for Researchers</h2>
              <p className="text-muted text-sm mt-1">Tools designed specifically for bioinformatics and scientific computing.</p>
            </div>
            <div className="space-y-2.5">
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-teal-900/30 flex items-center justify-center shrink-0"><BookOpen className="w-4 h-4 text-teal-600 dark:text-teal-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">181+ Protocols</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Pre-built analysis protocols for genomics, ML, visualization, and more. Activate one to guide Claude with domain-specific instructions. Create your own or generate them with AI.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-blue-900/30 flex items-center justify-center shrink-0"><AtSign className="w-4 h-4 text-blue-600 dark:text-blue-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">File Mentions</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Type <code className="text-[10px] bg-surface px-1 rounded">@</code> in the chat to reference files by name. Claude will know exactly which files you're talking about.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-emerald-900/30 flex items-center justify-center shrink-0"><BookMarked className="w-4 h-4 text-emerald-600 dark:text-emerald-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">PubMed Literature</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Toggle PubMed search in Ask or Report mode to ground answers in peer-reviewed articles with inline citations and references.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-elevated/30 flex items-center justify-center shrink-0"><Pencil className="w-4 h-4 text-secondary" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Edit & Save Files</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Click any file to preview it, then click <span className="font-medium text-secondary">Edit</span> to modify and <span className="font-medium text-secondary">Save</span> to write changes — works for both local and remote files.</p>
                </div>
              </div>
              <div className="flex items-start gap-3 p-3.5 bg-panel rounded-lg border border-border-default">
                <div className="w-8 h-8 rounded-lg bg-orange-900/30 flex items-center justify-center shrink-0"><GitBranch className="w-4 h-4 text-orange-600 dark:text-orange-400" /></div>
                <div>
                  <p className="text-sm font-medium text-primary">Git & GitHub</p>
                  <p className="text-xs text-muted mt-0.5 leading-relaxed">Built-in Git panel for staging, committing, branching, stash, and one-click GitHub publishing.</p>
                </div>
              </div>
            </div>
            <div className="flex gap-3">
              <button onClick={() => setStep('tour-remote')} className="flex items-center gap-1.5 px-3 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"><ArrowLeft className="w-3.5 h-3.5" /> Back</button>
              <button onClick={() => setStep('tour-shortcuts')} className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white rounded-lg font-medium transition-colors">Next: Tips <ArrowRight className="w-4 h-4" /></button>
            </div>
          </div>
        )}

        {/* ========== TOUR: TIPS & SHORTCUTS ========== */}
        {step === 'tour-shortcuts' && (
          <div className="space-y-5">
            <div className="text-center">
              <div className="w-12 h-12 rounded-xl bg-amber-900/30 flex items-center justify-center mx-auto mb-3">
                <Keyboard className="w-6 h-6 text-amber-600 dark:text-amber-400" />
              </div>
              <h2 className="text-lg font-semibold text-primary">Tips & Shortcuts</h2>
              <p className="text-muted text-sm mt-1">A few things to help you get the most out of Operon.</p>
            </div>
            <div className="space-y-2">
              <div className="flex items-start gap-3 p-3 bg-panel rounded-lg border border-border-default">
                <Keyboard className="w-4 h-4 text-secondary shrink-0 mt-0.5" />
                <p className="text-xs text-secondary"><kbd className="px-1.5 py-0.5 bg-surface rounded text-[10px] text-secondary font-mono">{modKey}+K</kbd> to start a new chat, <kbd className="px-1.5 py-0.5 bg-surface rounded text-[10px] text-secondary font-mono">{modKey}+S</kbd> to save files</p>
              </div>
              <div className="flex items-start gap-3 p-3 bg-panel rounded-lg border border-border-default">
                <Zap className="w-4 h-4 text-amber-600 dark:text-amber-400 shrink-0 mt-0.5" />
                <p className="text-xs text-secondary">Use <span className="font-medium text-amber-700 dark:text-amber-300">Plan</span> first, then switch to <span className="font-medium text-blue-700 dark:text-blue-300">Agent</span> to execute — Claude auto-tracks progress in the plan file.</p>
              </div>
              <div className="flex items-start gap-3 p-3 bg-panel rounded-lg border border-border-default">
                <FlaskConical className="w-4 h-4 text-purple-600 dark:text-purple-400 shrink-0 mt-0.5" />
                <p className="text-xs text-secondary"><span className="font-medium text-purple-700 dark:text-purple-300">Report mode</span> reads your analysis files and generates a formatted PDF report with methods, results, and PubMed references.</p>
              </div>
              <div className="flex items-start gap-3 p-3 bg-panel rounded-lg border border-border-default">
                <Mic className="w-4 h-4 text-red-600 dark:text-red-400 shrink-0 mt-0.5" />
                <p className="text-xs text-secondary">Click the <span className="font-medium text-red-700 dark:text-red-300">microphone</span> button to dictate prompts. Right-click files to delete or download them.</p>
              </div>
              <div className="flex items-start gap-3 p-3 bg-panel rounded-lg border border-border-default">
                <MonitorSmartphone className="w-4 h-4 text-blue-600 dark:text-blue-400 shrink-0 mt-0.5" />
                <p className="text-xs text-secondary">Drag panel borders to resize. Operon auto-updates — you'll always have the latest version.</p>
              </div>
            </div>
            <div className="flex gap-3">
              <button onClick={() => setStep('tour-features')} className="flex items-center gap-1.5 px-3 py-2.5 bg-surface hover:bg-elevated text-secondary rounded-lg transition-colors text-sm"><ArrowLeft className="w-3.5 h-3.5" /> Back</button>
              <button onClick={finishSetup} className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 bg-green-600 hover:bg-green-500 text-white rounded-lg font-medium transition-colors"><CheckCircle className="w-4 h-4" /> Start Using Operon</button>
            </div>
          </div>
        )}

        {/* ========== COMPLETE ========== */}
        {step === 'complete' && (
          <div className="text-center space-y-4">
            <div className="flex justify-center">
              <div className="w-14 h-14 rounded-full bg-green-900/30 flex items-center justify-center">
                <CheckCircle className="w-8 h-8 text-green-600 dark:text-green-400" />
              </div>
            </div>
            <div>
              <h2 className="text-lg font-semibold text-primary">You're all set!</h2>
              <p className="text-secondary text-sm mt-1">Operon is ready. Open a folder to get started.</p>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
