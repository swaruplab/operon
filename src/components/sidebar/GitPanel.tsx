import { useState, useEffect, useCallback } from 'react';
import {
  GitBranch,
  RefreshCw,
  Upload,
  Github,
  CheckCircle2,
  XCircle,
  AlertCircle,
  Download,
  Tag,
  Plus,
  FileEdit,
  FilePlus,
  FileX,
  Loader2,
  ChevronDown,
  ChevronRight,
  ExternalLink,
  LogIn,
  Lock,
  Globe,
  Link2,
  ArrowDownToLine,
  Search,
  Undo2,
  Archive,
  ArchiveRestore,
  Trash2,
  History,
  Copy,
  GitCommitHorizontal,
  MinusCircle,
  PlusCircle,
} from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useProject } from '../../context/ProjectContext';

// ── Types matching Rust structs ──

interface GitStatus {
  is_repo: boolean;
  branch: string;
  changed_files: number;
  staged_files: number;
  untracked_files: number;
  ahead: number;
  behind: number;
  remote_url: string;
  has_remote: boolean;
  last_commit_message: string;
  last_commit_time: string;
}

interface GhAuthStatus {
  installed: boolean;
  authenticated: boolean;
  username: string;
  scopes: string;
}

interface VersionInfo {
  current: string;
  next_patch: string;
  next_minor: string;
  next_major: string;
  total_commits: number;
}

interface GhRepo {
  name: string;
  full_name: string;
  private: boolean;
  url: string;
  description: string;
}

interface BranchInfo {
  current: string;
  branches: string[];
  remote_branches: string[];
}

interface ChangedFile {
  path: string;
  status: string;
  staged: boolean;
}

interface StashEntry {
  index: number;
  message: string;
  date: string;
}

interface CommitEntry {
  hash: string;
  short_hash: string;
  message: string;
  author: string;
  date: string;
  files_changed: number;
}

// ── Component ──

export function GitPanel() {
  const { projectPath } = useProject();

  // State
  const [gitStatus, setGitStatus] = useState<GitStatus | null>(null);
  const [ghAuth, setGhAuth] = useState<GhAuthStatus | null>(null);
  const [versionInfo, setVersionInfo] = useState<VersionInfo | null>(null);
  const [branchInfo, setBranchInfo] = useState<BranchInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [publishing, setPublishing] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [publishMessage, setPublishMessage] = useState('');
  const [autoVersion, setAutoVersion] = useState(true);
  const [versionBump, setVersionBump] = useState<'patch' | 'minor' | 'major' | 'custom'>('patch');
  const [customVersion, setCustomVersion] = useState('');
  const [pushTargetBranch, setPushTargetBranch] = useState<string | null>(null); // null = same as local
  const [showPushBranchPicker, setShowPushBranchPicker] = useState(false);
  const [statusMessage, setStatusMessage] = useState<{ type: 'success' | 'error'; text: string } | null>(null);
  const [changesExpanded, setChangesExpanded] = useState(true);
  const [ghSetupStep, setGhSetupStep] = useState<'idle' | 'installing' | 'logging-in' | 'creating-repo' | 'linking-repo'>('idle');
  const [loginCode, setLoginCode] = useState<string | null>(null);

  // Repo creation / linking state
  const [changingRemote, setChangingRemote] = useState(false);
  const [repoMode, setRepoMode] = useState<'new' | 'existing'>('new');
  const [newRepoName, setNewRepoName] = useState('');
  const [newRepoDescription, setNewRepoDescription] = useState('');
  const [newRepoPrivate, setNewRepoPrivate] = useState(true);
  const [existingRepos, setExistingRepos] = useState<GhRepo[]>([]);
  const [existingRepoSearch, setExistingRepoSearch] = useState('');
  const [loadingRepos, setLoadingRepos] = useState(false);
  const [selectedRepo, setSelectedRepo] = useState<GhRepo | null>(null);

  // Branch UI
  const [showBranchPicker, setShowBranchPicker] = useState(false);
  const [newBranchName, setNewBranchName] = useState('');
  const [creatingBranch, setCreatingBranch] = useState(false);

  // File-level staging
  const [changedFiles, setChangedFiles] = useState<ChangedFile[]>([]);

  // Stash
  const [stashes, setStashes] = useState<StashEntry[]>([]);
  const [showStash, setShowStash] = useState(false);
  const [stashMessage, setStashMessage] = useState('');

  // Commit history
  const [commits, setCommits] = useState<CommitEntry[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const [selectedCommit, setSelectedCommit] = useState<string | null>(null);
  const [commitDetail, setCommitDetail] = useState<string | null>(null);

  // Amend
  const [showAmend, setShowAmend] = useState(false);
  const [amendMessage, setAmendMessage] = useState('');

  // ── Data loading ──

  const refresh = useCallback(async () => {
    if (!projectPath) return;
    setLoading(true);
    try {
      const [status, auth] = await Promise.all([
        invoke<GitStatus>('git_status', { projectPath }),
        invoke<GhAuthStatus>('gh_check_auth'),
      ]);
      setGitStatus(status);
      setGhAuth(auth);

      if (status.is_repo) {
        try {
          const [ver, branches, files] = await Promise.all([
            invoke<VersionInfo>('git_version_info', { projectPath }),
            invoke<BranchInfo>('git_list_branches', { projectPath }),
            invoke<ChangedFile[]>('git_changed_files', { projectPath }),
          ]);
          setVersionInfo(ver);
          setBranchInfo(branches);
          setChangedFiles(files);
        } catch {
          setVersionInfo(null);
          setBranchInfo(null);
          setChangedFiles([]);
        }
      }

      // Set default repo name from folder
      if (!newRepoName) {
        const folderName = projectPath.split('/').pop() || 'my-project';
        setNewRepoName(folderName);
      }
    } catch (err) {
      console.error('Git refresh failed:', err);
    }
    setLoading(false);
  }, [projectPath]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // ── Actions ──

  const initRepo = async () => {
    if (!projectPath) return;
    try {
      await invoke('git_init', { projectPath });
      setStatusMessage({ type: 'success', text: 'Git repository initialized!' });
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `Init failed: ${err}` });
    }
  };

  const installGh = async () => {
    setGhSetupStep('installing');
    try {
      await invoke('gh_install');
      setStatusMessage({ type: 'success', text: 'GitHub CLI installed!' });
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
    setGhSetupStep('idle');
  };

  // Listen for gh login events
  useEffect(() => {
    const unlistenCode = listen<string>('gh-login-code', (event) => {
      setLoginCode(event.payload);
    });
    const unlistenDone = listen<boolean>('gh-login-done', (event) => {
      setLoginCode(null);
      setGhSetupStep('idle');
      if (event.payload) {
        setStatusMessage({ type: 'success', text: 'Logged in to GitHub!' });
      } else {
        setStatusMessage({ type: 'error', text: 'Login was not completed. Please try again.' });
      }
      refresh();
    });
    return () => {
      unlistenCode.then((u) => u());
      unlistenDone.then((u) => u());
    };
  }, [refresh]);

  const loginGh = async () => {
    setGhSetupStep('logging-in');
    setLoginCode(null);
    try {
      const result = await invoke<string>('gh_login');
      if (result === 'ALREADY_AUTHED') {
        setStatusMessage({ type: 'success', text: 'Already logged in to GitHub!' });
        setGhSetupStep('idle');
        refresh();
      }
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
      setGhSetupStep('idle');
    }
  };

  const loadExistingRepos = async () => {
    setLoadingRepos(true);
    try {
      const repos = await invoke<GhRepo[]>('gh_list_repos');
      setExistingRepos(repos);
    } catch (err) {
      setStatusMessage({ type: 'error', text: `Failed to load repos: ${err}` });
    }
    setLoadingRepos(false);
  };

  const createRepo = async () => {
    if (!projectPath || !newRepoName.trim()) return;
    setGhSetupStep('creating-repo');
    try {
      await invoke<string>('gh_create_repo', {
        projectPath,
        repoName: newRepoName.trim(),
        private: newRepoPrivate,
        description: newRepoDescription.trim(),
      });
      setStatusMessage({ type: 'success', text: `Repository "${newRepoName}" created on GitHub!` });
      setChangingRemote(false);
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
    setGhSetupStep('idle');
  };

  const linkExistingRepo = async () => {
    if (!projectPath || !selectedRepo) return;
    setGhSetupStep('linking-repo');
    try {
      const url = selectedRepo.url.endsWith('.git') ? selectedRepo.url : `${selectedRepo.url}.git`;
      await invoke('gh_add_remote', { projectPath, remoteUrl: url });
      setStatusMessage({ type: 'success', text: `Linked to ${selectedRepo.full_name}` });
      setSelectedRepo(null);
      setChangingRemote(false);
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
    setGhSetupStep('idle');
  };

  const switchBranch = async (branch: string) => {
    if (!projectPath) return;
    try {
      await invoke('git_switch_branch', { projectPath, branch, create: false });
      setShowBranchPicker(false);
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
  };

  const createBranch = async () => {
    if (!projectPath || !newBranchName.trim()) return;
    setCreatingBranch(true);
    try {
      await invoke('git_switch_branch', { projectPath, branch: newBranchName.trim(), create: true });
      setNewBranchName('');
      setShowBranchPicker(false);
      setStatusMessage({ type: 'success', text: `Created and switched to branch "${newBranchName.trim()}"` });
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
    setCreatingBranch(false);
  };

  const pullChanges = async () => {
    if (!projectPath) return;
    setPulling(true);
    try {
      const result = await invoke<string>('git_pull', { projectPath });
      setStatusMessage({ type: 'success', text: result || 'Already up to date.' });
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `Pull failed: ${err}` });
    }
    setPulling(false);
  };

  const publish = async () => {
    if (!projectPath) return;
    const message = publishMessage.trim() || `Update ${new Date().toLocaleDateString()}`;
    setPublishing(true);
    setStatusMessage(null);

    // Determine version tag
    let versionTag: string | null = null;
    if (autoVersion && versionInfo) {
      if (versionBump === 'patch') versionTag = versionInfo.next_patch;
      else if (versionBump === 'minor') versionTag = versionInfo.next_minor;
      else if (versionBump === 'major') versionTag = versionInfo.next_major;
      else if (versionBump === 'custom' && customVersion.trim()) versionTag = customVersion.trim();
    }

    try {
      const result = await invoke<string>('git_publish', {
        projectPath,
        message,
        autoVersion,
        versionTag,
        targetBranch: pushTargetBranch,
      });
      setStatusMessage({ type: 'success', text: result });
      setPublishMessage('');
      refresh();
    } catch (err) {
      setStatusMessage({ type: 'error', text: `${err}` });
    }
    setPublishing(false);
  };

  // ── File staging actions ──

  const stageFile = async (path: string) => {
    if (!projectPath) return;
    try {
      await invoke('git_stage_files', { projectPath, paths: [path] });
      const files = await invoke<ChangedFile[]>('git_changed_files', { projectPath });
      setChangedFiles(files);
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const unstageFile = async (path: string) => {
    if (!projectPath) return;
    try {
      await invoke('git_unstage_files', { projectPath, paths: [path] });
      const files = await invoke<ChangedFile[]>('git_changed_files', { projectPath });
      setChangedFiles(files);
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const discardFile = async (path: string) => {
    if (!projectPath) return;
    try {
      await invoke('git_discard_files', { projectPath, paths: [path] });
      const files = await invoke<ChangedFile[]>('git_changed_files', { projectPath });
      setChangedFiles(files);
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const stageAll = async () => {
    if (!projectPath) return;
    const unstaged = changedFiles.filter(f => !f.staged).map(f => f.path);
    if (unstaged.length === 0) return;
    try {
      await invoke('git_stage_files', { projectPath, paths: unstaged });
      const files = await invoke<ChangedFile[]>('git_changed_files', { projectPath });
      setChangedFiles(files);
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const unstageAll = async () => {
    if (!projectPath) return;
    const staged = changedFiles.filter(f => f.staged).map(f => f.path);
    if (staged.length === 0) return;
    try {
      await invoke('git_unstage_files', { projectPath, paths: staged });
      const files = await invoke<ChangedFile[]>('git_changed_files', { projectPath });
      setChangedFiles(files);
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  // ── Stash actions ──

  const loadStashes = async () => {
    if (!projectPath) return;
    try {
      const list = await invoke<StashEntry[]>('git_stash_list', { projectPath });
      setStashes(list);
    } catch { setStashes([]); }
  };

  const saveStash = async () => {
    if (!projectPath) return;
    try {
      await invoke('git_stash_save', { projectPath, message: stashMessage || null });
      setStashMessage('');
      setStatusMessage({ type: 'success', text: 'Changes stashed!' });
      loadStashes();
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const popStash = async (index: number) => {
    if (!projectPath) return;
    try {
      await invoke('git_stash_pop', { projectPath, index });
      setStatusMessage({ type: 'success', text: 'Stash applied and dropped' });
      loadStashes();
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  const dropStash = async (index: number) => {
    if (!projectPath) return;
    try {
      await invoke('git_stash_drop', { projectPath, index });
      loadStashes();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  // ── History actions ──

  const loadHistory = async () => {
    if (!projectPath) return;
    try {
      const log = await invoke<CommitEntry[]>('git_log', { projectPath, count: 30 });
      setCommits(log);
    } catch { setCommits([]); }
  };

  const viewCommit = async (hash: string) => {
    if (!projectPath) return;
    if (selectedCommit === hash) { setSelectedCommit(null); setCommitDetail(null); return; }
    setSelectedCommit(hash);
    try {
      const detail = await invoke<string>('git_show_commit', { projectPath, hash });
      setCommitDetail(detail);
    } catch { setCommitDetail('Failed to load commit details'); }
  };

  // ── Amend ──

  const amendCommit = async () => {
    if (!projectPath) return;
    try {
      await invoke('git_amend', { projectPath, message: amendMessage || null });
      setShowAmend(false);
      setAmendMessage('');
      setStatusMessage({ type: 'success', text: 'Last commit amended' });
      refresh();
    } catch (err) { setStatusMessage({ type: 'error', text: `${err}` }); }
  };

  // Auto-dismiss status messages
  useEffect(() => {
    if (statusMessage) {
      const timer = setTimeout(() => setStatusMessage(null), 6000);
      return () => clearTimeout(timer);
    }
  }, [statusMessage]);

  // ── Render helpers ──

  const stagedFiles = changedFiles.filter(f => f.staged);
  const unstagedFiles = changedFiles.filter(f => !f.staged);

  const totalChanges = (gitStatus?.changed_files || 0) + (gitStatus?.staged_files || 0) + (gitStatus?.untracked_files || 0);
  const filteredRepos = existingRepos.filter(r =>
    !existingRepoSearch || r.full_name.toLowerCase().includes(existingRepoSearch.toLowerCase())
      || r.description?.toLowerCase().includes(existingRepoSearch.toLowerCase())
  );

  if (!projectPath) {
    return (
      <div className="flex flex-col h-full bg-zinc-900">
        <Header />
        <div className="flex-1 flex items-center justify-center px-4">
          <p className="text-zinc-500 text-sm text-center">Open a project folder to use Git</p>
        </div>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex flex-col h-full bg-zinc-900">
        <Header />
        <div className="flex-1 flex items-center justify-center">
          <Loader2 className="w-5 h-5 text-zinc-500 animate-spin" />
        </div>
      </div>
    );
  }

  // ── Not a git repo yet → offer to init ──
  if (!gitStatus?.is_repo) {
    return (
      <div className="flex flex-col h-full bg-zinc-900">
        <Header onRefresh={refresh} />
        <div className="flex-1 flex flex-col items-center justify-center px-4 gap-4">
          <GitBranch className="w-10 h-10 text-zinc-600" />
          <p className="text-zinc-400 text-sm text-center">
            This folder isn't a Git repository yet.
          </p>
          <button
            onClick={initRepo}
            className="flex items-center gap-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 rounded-lg text-sm text-white font-medium transition-colors"
          >
            <Plus className="w-4 h-4" />
            Initialize Repository
          </button>
        </div>
        <StatusToast message={statusMessage} />
      </div>
    );
  }

  // ── GitHub not set up (or user is changing remote) → guided setup ──
  const needsGhSetup = !ghAuth?.installed || !ghAuth?.authenticated || !gitStatus?.has_remote || changingRemote;

  return (
    <div className="flex flex-col h-full bg-zinc-900">
      <Header onRefresh={refresh} />

      <div className="flex-1 overflow-y-auto">
        {/* Branch info + switcher */}
        <div className="px-3 py-2 border-b border-zinc-800/50">
          <div className="flex items-center gap-2">
            <button
              onClick={() => {
                setShowBranchPicker(!showBranchPicker);
              }}
              className="flex items-center gap-1.5 hover:bg-zinc-800 rounded px-1.5 py-0.5 transition-colors"
              title="Switch branch"
            >
              <GitBranch className="w-3.5 h-3.5 text-blue-400" />
              <span className="text-sm text-zinc-200 font-medium">{gitStatus.branch || 'main'}</span>
              <ChevronDown className="w-3 h-3 text-zinc-500" />
            </button>
            {gitStatus.ahead > 0 && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-900/40 text-blue-300">
                ↑{gitStatus.ahead}
              </span>
            )}
            {gitStatus.behind > 0 && (
              <button
                onClick={pullChanges}
                disabled={pulling}
                className="text-[10px] px-1.5 py-0.5 rounded bg-orange-900/40 text-orange-300 hover:bg-orange-800/50 transition-colors"
                title="Pull changes"
              >
                {pulling ? <Loader2 className="w-3 h-3 animate-spin inline" /> : `↓${gitStatus.behind}`}
              </button>
            )}
            {gitStatus.has_remote && gitStatus.behind === 0 && gitStatus.ahead === 0 && (
              <button
                onClick={pullChanges}
                disabled={pulling}
                className="ml-auto p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
                title="Pull from remote"
              >
                {pulling ? <Loader2 className="w-3 h-3 animate-spin" /> : <ArrowDownToLine className="w-3 h-3" />}
              </button>
            )}
          </div>
          {gitStatus.last_commit_message && (
            <p className="text-[11px] text-zinc-500 mt-1 truncate">
              {gitStatus.last_commit_message} · {gitStatus.last_commit_time}
            </p>
          )}

          {/* Branch picker dropdown */}
          {showBranchPicker && branchInfo && (
            <div className="mt-2 bg-zinc-800 border border-zinc-700 rounded-lg overflow-hidden">
              <div className="max-h-52 overflow-y-auto">
                {/* Local branches */}
                {branchInfo.branches.length > 0 && (
                  <div className="px-2.5 pt-1.5 pb-0.5">
                    <span className="text-[9px] text-zinc-500 uppercase tracking-wider font-semibold">Local</span>
                  </div>
                )}
                {branchInfo.branches.map((b) => (
                  <button
                    key={`local-${b}`}
                    onClick={() => switchBranch(b)}
                    className={`w-full flex items-center gap-2 px-3 py-1.5 text-[11px] hover:bg-zinc-700 transition-colors text-left ${
                      b === branchInfo.current ? 'text-blue-300 bg-blue-900/20' : 'text-zinc-300'
                    }`}
                  >
                    <GitBranch className="w-3 h-3 shrink-0" />
                    {b}
                    {b === branchInfo.current && <CheckCircle2 className="w-3 h-3 text-blue-400 ml-auto" />}
                  </button>
                ))}

                {/* Remote-only branches */}
                {branchInfo.remote_branches.length > 0 && (
                  <>
                    <div className="px-2.5 pt-2 pb-0.5 border-t border-zinc-700/50">
                      <span className="text-[9px] text-zinc-500 uppercase tracking-wider font-semibold">Remote</span>
                    </div>
                    {branchInfo.remote_branches.map((b) => (
                      <button
                        key={`remote-${b}`}
                        onClick={() => switchBranch(b)}
                        className="w-full flex items-center gap-2 px-3 py-1.5 text-[11px] text-zinc-400 hover:bg-zinc-700 hover:text-zinc-200 transition-colors text-left"
                      >
                        <Globe className="w-3 h-3 shrink-0 text-zinc-600" />
                        {b}
                      </button>
                    ))}
                  </>
                )}
              </div>
              <div className="border-t border-zinc-700 p-2">
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={newBranchName}
                    onChange={(e) => setNewBranchName(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') createBranch(); }}
                    placeholder="New branch name..."
                    className="flex-1 bg-zinc-900 border border-zinc-600 rounded px-2 py-1 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-500"
                  />
                  <button
                    onClick={createBranch}
                    disabled={creatingBranch || !newBranchName.trim()}
                    className="px-2 py-1 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 rounded text-[11px] text-white font-medium transition-colors"
                  >
                    {creatingBranch ? <Loader2 className="w-3 h-3 animate-spin" /> : 'Create'}
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* GitHub setup guide */}
        {needsGhSetup && (
          <div className="px-3 py-3 border-b border-zinc-800/50">
            <div className="bg-zinc-800/60 rounded-lg p-3 space-y-3">
              <div className="flex items-center gap-2">
                <Github className="w-4 h-4 text-zinc-300" />
                <span className="text-xs font-semibold text-zinc-300">Connect to GitHub</span>
              </div>

              {/* Step 1: Install gh CLI */}
              <SetupStep
                number={1}
                label="GitHub CLI"
                done={ghAuth?.installed ?? false}
                active={!ghAuth?.installed}
              >
                {!ghAuth?.installed && (
                  <button
                    onClick={installGh}
                    disabled={ghSetupStep !== 'idle'}
                    className="mt-1.5 w-full flex items-center justify-center gap-1.5 px-3 py-1.5 bg-zinc-700 hover:bg-zinc-600 disabled:opacity-50 rounded text-xs text-zinc-200 transition-colors"
                  >
                    {ghSetupStep === 'installing' ? (
                      <><Loader2 className="w-3 h-3 animate-spin" /> Installing...</>
                    ) : (
                      <><Download className="w-3 h-3" /> Install GitHub CLI</>
                    )}
                  </button>
                )}
              </SetupStep>

              {/* Step 2: Login */}
              <SetupStep
                number={2}
                label="GitHub Account"
                done={ghAuth?.authenticated ?? false}
                active={(ghAuth?.installed ?? false) && !(ghAuth?.authenticated ?? false)}
              >
                {ghAuth?.installed && !ghAuth?.authenticated && !loginCode && (
                  <button
                    onClick={loginGh}
                    disabled={ghSetupStep !== 'idle'}
                    className="mt-1.5 w-full flex items-center justify-center gap-1.5 px-3 py-1.5 bg-zinc-700 hover:bg-zinc-600 disabled:opacity-50 rounded text-xs text-zinc-200 transition-colors"
                  >
                    {ghSetupStep === 'logging-in' ? (
                      <><Loader2 className="w-3 h-3 animate-spin" /> Starting...</>
                    ) : (
                      <><LogIn className="w-3 h-3" /> Sign in with GitHub</>
                    )}
                  </button>
                )}
                {loginCode && (
                  <div className="mt-2 space-y-2">
                    <p className="text-[11px] text-zinc-400">Enter this code on GitHub:</p>
                    <div
                      className="flex items-center justify-center gap-2 py-2.5 px-3 bg-zinc-800 border border-zinc-600 rounded-lg cursor-pointer hover:border-zinc-500 transition-colors"
                      onClick={() => {
                        navigator.clipboard.writeText(loginCode);
                        setStatusMessage({ type: 'success', text: 'Code copied to clipboard!' });
                      }}
                      title="Click to copy"
                    >
                      <span className="text-xl font-mono font-bold text-white tracking-[0.25em]">{loginCode}</span>
                    </div>
                    <p className="text-[10px] text-zinc-500 text-center">Click the code to copy · Complete sign-in in your browser</p>
                    <div className="flex items-center justify-center gap-1.5 text-[11px] text-blue-400">
                      <Loader2 className="w-3 h-3 animate-spin" />
                      Waiting for authorization...
                    </div>
                  </div>
                )}
                {ghAuth?.authenticated && ghAuth.username && (
                  <p className="text-[11px] text-green-400 mt-1">Signed in as @{ghAuth.username}</p>
                )}
              </SetupStep>

              {/* Step 3: Create or link repo */}
              <SetupStep
                number={3}
                label="GitHub Repository"
                done={gitStatus.has_remote && !changingRemote}
                active={(ghAuth?.authenticated ?? false) && (!gitStatus.has_remote || changingRemote)}
              >
                {ghAuth?.authenticated && (!gitStatus.has_remote || changingRemote) && (
                  <div className="mt-2 space-y-3">
                    {/* Mode toggle: New vs Existing */}
                    <div className="flex border border-zinc-600 rounded-lg overflow-hidden">
                      <button
                        onClick={() => setRepoMode('new')}
                        className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-[11px] font-medium transition-colors ${
                          repoMode === 'new'
                            ? 'bg-green-700 text-white'
                            : 'bg-zinc-800 text-zinc-400 hover:text-zinc-200'
                        }`}
                      >
                        <Plus className="w-3 h-3" />
                        New Repo
                      </button>
                      <button
                        onClick={() => {
                          setRepoMode('existing');
                          if (existingRepos.length === 0) loadExistingRepos();
                        }}
                        className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 text-[11px] font-medium transition-colors ${
                          repoMode === 'existing'
                            ? 'bg-blue-700 text-white'
                            : 'bg-zinc-800 text-zinc-400 hover:text-zinc-200'
                        }`}
                      >
                        <Link2 className="w-3 h-3" />
                        Existing Repo
                      </button>
                    </div>

                    {/* New repo form */}
                    {repoMode === 'new' && (
                      <div className="space-y-2">
                        <input
                          type="text"
                          value={newRepoName}
                          onChange={(e) => setNewRepoName(e.target.value)}
                          placeholder="Repository name"
                          className="w-full bg-zinc-900 border border-zinc-600 rounded-lg px-2.5 py-1.5 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-green-500"
                        />
                        <input
                          type="text"
                          value={newRepoDescription}
                          onChange={(e) => setNewRepoDescription(e.target.value)}
                          placeholder="Description (optional)"
                          className="w-full bg-zinc-900 border border-zinc-600 rounded-lg px-2.5 py-1.5 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-green-500"
                        />

                        {/* Visibility toggle */}
                        <div className="flex gap-2">
                          <button
                            onClick={() => setNewRepoPrivate(true)}
                            className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-[11px] font-medium border transition-colors ${
                              newRepoPrivate
                                ? 'border-amber-600 bg-amber-900/30 text-amber-300'
                                : 'border-zinc-700 bg-zinc-800 text-zinc-400 hover:border-zinc-600'
                            }`}
                          >
                            <Lock className="w-3 h-3" />
                            Private
                          </button>
                          <button
                            onClick={() => setNewRepoPrivate(false)}
                            className={`flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-lg text-[11px] font-medium border transition-colors ${
                              !newRepoPrivate
                                ? 'border-green-600 bg-green-900/30 text-green-300'
                                : 'border-zinc-700 bg-zinc-800 text-zinc-400 hover:border-zinc-600'
                            }`}
                          >
                            <Globe className="w-3 h-3" />
                            Public
                          </button>
                        </div>

                        <button
                          onClick={createRepo}
                          disabled={ghSetupStep !== 'idle' || !newRepoName.trim()}
                          className="w-full flex items-center justify-center gap-1.5 px-3 py-2 bg-green-600 hover:bg-green-500 disabled:opacity-50 disabled:cursor-not-allowed rounded-lg text-xs text-white font-medium transition-colors"
                        >
                          {ghSetupStep === 'creating-repo' ? (
                            <><Loader2 className="w-3.5 h-3.5 animate-spin" /> Creating...</>
                          ) : (
                            <><Github className="w-3.5 h-3.5" /> Create &amp; Push</>
                          )}
                        </button>
                      </div>
                    )}

                    {/* Existing repo picker */}
                    {repoMode === 'existing' && (
                      <div className="space-y-2">
                        {loadingRepos ? (
                          <div className="flex items-center justify-center py-4">
                            <Loader2 className="w-4 h-4 text-zinc-500 animate-spin" />
                          </div>
                        ) : (
                          <>
                            <div className="relative">
                              <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-zinc-600" />
                              <input
                                type="text"
                                value={existingRepoSearch}
                                onChange={(e) => setExistingRepoSearch(e.target.value)}
                                placeholder="Search your repos..."
                                className="w-full bg-zinc-900 border border-zinc-600 rounded-lg pl-7 pr-2.5 py-1.5 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-500"
                              />
                            </div>
                            <div className="max-h-40 overflow-y-auto border border-zinc-700 rounded-lg">
                              {filteredRepos.length === 0 ? (
                                <p className="text-[11px] text-zinc-600 py-3 text-center">
                                  {existingRepos.length === 0 ? 'No repos found' : 'No matches'}
                                </p>
                              ) : (
                                filteredRepos.map((r) => (
                                  <button
                                    key={r.full_name}
                                    onClick={() => setSelectedRepo(r)}
                                    className={`w-full flex items-start gap-2 px-2.5 py-1.5 text-left hover:bg-zinc-800 transition-colors border-b border-zinc-800/50 last:border-b-0 ${
                                      selectedRepo?.full_name === r.full_name ? 'bg-blue-900/30' : ''
                                    }`}
                                  >
                                    {r.private ? (
                                      <Lock className="w-3 h-3 text-amber-400 mt-0.5 shrink-0" />
                                    ) : (
                                      <Globe className="w-3 h-3 text-green-400 mt-0.5 shrink-0" />
                                    )}
                                    <div className="min-w-0">
                                      <span className="text-[11px] text-zinc-200 font-medium block truncate">
                                        {r.full_name}
                                      </span>
                                      {r.description && (
                                        <span className="text-[10px] text-zinc-500 block truncate">
                                          {r.description}
                                        </span>
                                      )}
                                    </div>
                                    {selectedRepo?.full_name === r.full_name && (
                                      <CheckCircle2 className="w-3.5 h-3.5 text-blue-400 ml-auto mt-0.5 shrink-0" />
                                    )}
                                  </button>
                                ))
                              )}
                            </div>

                            <button
                              onClick={linkExistingRepo}
                              disabled={!selectedRepo || ghSetupStep === 'linking-repo'}
                              className="w-full flex items-center justify-center gap-1.5 px-3 py-2 bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed rounded-lg text-xs text-white font-medium transition-colors"
                            >
                              {ghSetupStep === 'linking-repo' ? (
                                <><Loader2 className="w-3.5 h-3.5 animate-spin" /> Linking...</>
                              ) : (
                                <><Link2 className="w-3.5 h-3.5" /> Link Repository</>
                              )}
                            </button>
                          </>
                        )}
                      </div>
                    )}
                  </div>
                )}
                {gitStatus.has_remote && !changingRemote && (
                  <p className="text-[11px] text-zinc-500 mt-1 truncate" title={gitStatus.remote_url}>
                    {gitStatus.remote_url.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}
                  </p>
                )}
                {changingRemote && (
                  <div className="mt-2 flex items-center gap-2">
                    <p className="text-[10px] text-zinc-500 flex-1">
                      Current: <span className="text-zinc-400">{gitStatus.remote_url.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}</span>
                    </p>
                    <button
                      onClick={() => setChangingRemote(false)}
                      className="text-[10px] text-zinc-500 hover:text-zinc-300 underline"
                    >
                      Cancel
                    </button>
                  </div>
                )}
              </SetupStep>
            </div>
          </div>
        )}

        {/* Connected status */}
        {ghAuth?.authenticated && gitStatus.has_remote && !changingRemote && (
          <div className="px-3 py-2 border-b border-zinc-800/50">
            <div className="flex items-center gap-2">
              <CheckCircle2 className="w-3.5 h-3.5 text-green-500 shrink-0" />
              <span className="text-[11px] text-green-400 shrink-0">
                Connected · @{ghAuth.username}
              </span>
              <span className="text-[10px] text-zinc-600 truncate flex-1 text-right" title={gitStatus.remote_url}>
                {gitStatus.remote_url.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}
              </span>
              <a
                href="#"
                onClick={(e) => {
                  e.preventDefault();
                  const url = gitStatus.remote_url.replace(/\.git$/, '');
                  if (url.startsWith('http')) {
                    window.open(url, '_blank');
                  }
                }}
              className="text-zinc-500 hover:text-zinc-300 shrink-0"
              title="Open on GitHub"
            >
              <ExternalLink className="w-3 h-3" />
            </a>
            </div>
            <button
              onClick={() => {
                setChangingRemote(true);
                if (existingRepos.length === 0) loadExistingRepos();
              }}
              className="mt-1.5 text-[10px] text-zinc-500 hover:text-blue-400 transition-colors"
            >
              Change repository...
            </button>
          </div>
        )}

        {/* Changes list — file-level staging */}
        <div className="border-b border-zinc-800/50">
          <button
            onClick={() => setChangesExpanded((v) => !v)}
            className="w-full flex items-center gap-1.5 px-3 py-2 hover:bg-zinc-800/40 transition-colors"
          >
            {changesExpanded ? (
              <ChevronDown className="w-3 h-3 text-zinc-500" />
            ) : (
              <ChevronRight className="w-3 h-3 text-zinc-500" />
            )}
            <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
              Changes
            </span>
            {totalChanges > 0 && (
              <span className="ml-auto text-[10px] px-1.5 py-0.5 rounded-full bg-blue-900/40 text-blue-300 font-medium">
                {totalChanges}
              </span>
            )}
          </button>

          {changesExpanded && (
            <div className="pb-1">
              {changedFiles.length === 0 ? (
                <p className="text-[11px] text-zinc-600 py-2 px-3">No changes</p>
              ) : (
                <>
                  {/* Staged files */}
                  {stagedFiles.length > 0 && (
                    <div>
                      <div className="flex items-center justify-between px-3 py-1">
                        <span className="text-[9px] text-green-500 uppercase tracking-wider font-semibold">
                          Staged ({stagedFiles.length})
                        </span>
                        <button onClick={unstageAll} className="text-[9px] text-zinc-500 hover:text-zinc-300" title="Unstage all">
                          <MinusCircle className="w-3 h-3" />
                        </button>
                      </div>
                      {stagedFiles.map((f) => (
                        <FileRow key={`s-${f.path}`} file={f} onStage={() => unstageFile(f.path)} onDiscard={() => discardFile(f.path)} staged />
                      ))}
                    </div>
                  )}

                  {/* Unstaged files */}
                  {unstagedFiles.length > 0 && (
                    <div>
                      <div className="flex items-center justify-between px-3 py-1">
                        <span className="text-[9px] text-yellow-500 uppercase tracking-wider font-semibold">
                          Changes ({unstagedFiles.length})
                        </span>
                        <button onClick={stageAll} className="text-[9px] text-zinc-500 hover:text-zinc-300" title="Stage all">
                          <PlusCircle className="w-3 h-3" />
                        </button>
                      </div>
                      {unstagedFiles.map((f) => (
                        <FileRow key={`u-${f.path}`} file={f} onStage={() => stageFile(f.path)} onDiscard={() => discardFile(f.path)} staged={false} />
                      ))}
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>

        {/* Stash section */}
        <div className="border-b border-zinc-800/50">
          <button
            onClick={() => { setShowStash(!showStash); if (!showStash) loadStashes(); }}
            className="w-full flex items-center gap-1.5 px-3 py-2 hover:bg-zinc-800/40 transition-colors"
          >
            {showStash ? <ChevronDown className="w-3 h-3 text-zinc-500" /> : <ChevronRight className="w-3 h-3 text-zinc-500" />}
            <Archive className="w-3 h-3 text-zinc-500" />
            <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">Stash</span>
            {stashes.length > 0 && (
              <span className="ml-auto text-[10px] px-1.5 py-0.5 rounded-full bg-zinc-700 text-zinc-400 font-medium">{stashes.length}</span>
            )}
          </button>
          {showStash && (
            <div className="px-3 pb-2 space-y-2">
              {totalChanges > 0 && (
                <div className="flex gap-1.5">
                  <input
                    type="text"
                    value={stashMessage}
                    onChange={(e) => setStashMessage(e.target.value)}
                    onKeyDown={(e) => { if (e.key === 'Enter') saveStash(); }}
                    placeholder="Stash message (optional)..."
                    className="flex-1 bg-zinc-900 border border-zinc-700 rounded px-2 py-1 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-500"
                  />
                  <button
                    onClick={saveStash}
                    className="px-2 py-1 bg-zinc-700 hover:bg-zinc-600 rounded text-[11px] text-zinc-200 transition-colors"
                    title="Stash changes"
                  >
                    <Archive className="w-3 h-3" />
                  </button>
                </div>
              )}
              {stashes.length === 0 ? (
                <p className="text-[10px] text-zinc-600">No stashed changes</p>
              ) : (
                stashes.map((s) => (
                  <div key={s.index} className="flex items-center gap-2 py-1 group">
                    <div className="flex-1 min-w-0">
                      <p className="text-[11px] text-zinc-300 truncate">{s.message}</p>
                      <p className="text-[9px] text-zinc-600">{s.date}</p>
                    </div>
                    <button onClick={() => popStash(s.index)} className="p-0.5 text-zinc-600 hover:text-green-400 opacity-0 group-hover:opacity-100 transition-all" title="Apply stash">
                      <ArchiveRestore className="w-3 h-3" />
                    </button>
                    <button onClick={() => dropStash(s.index)} className="p-0.5 text-zinc-600 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all" title="Drop stash">
                      <Trash2 className="w-3 h-3" />
                    </button>
                  </div>
                ))
              )}
            </div>
          )}
        </div>

        {/* Commit history */}
        <div className="border-b border-zinc-800/50">
          <button
            onClick={() => { setShowHistory(!showHistory); if (!showHistory) loadHistory(); }}
            className="w-full flex items-center gap-1.5 px-3 py-2 hover:bg-zinc-800/40 transition-colors"
          >
            {showHistory ? <ChevronDown className="w-3 h-3 text-zinc-500" /> : <ChevronRight className="w-3 h-3 text-zinc-500" />}
            <History className="w-3 h-3 text-zinc-500" />
            <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">History</span>
          </button>
          {showHistory && (
            <div className="pb-1 max-h-60 overflow-y-auto">
              {commits.length === 0 ? (
                <p className="text-[10px] text-zinc-600 px-3 py-2">No commits yet</p>
              ) : (
                commits.map((c) => (
                  <div key={c.hash}>
                    <button
                      onClick={() => viewCommit(c.hash)}
                      className={`w-full flex items-start gap-2 px-3 py-1.5 text-left hover:bg-zinc-800/40 transition-colors ${
                        selectedCommit === c.hash ? 'bg-zinc-800/60' : ''
                      }`}
                    >
                      <GitCommitHorizontal className="w-3 h-3 text-zinc-600 mt-0.5 shrink-0" />
                      <div className="flex-1 min-w-0">
                        <p className="text-[11px] text-zinc-300 truncate">{c.message}</p>
                        <p className="text-[9px] text-zinc-600">
                          {c.short_hash} · {c.author} · {c.date}
                          {c.files_changed > 0 && ` · ${c.files_changed} files`}
                        </p>
                      </div>
                      <button
                        onClick={(e) => { e.stopPropagation(); navigator.clipboard.writeText(c.hash); setStatusMessage({ type: 'success', text: 'Hash copied' }); }}
                        className="p-0.5 text-zinc-700 hover:text-zinc-400 transition-colors shrink-0"
                        title="Copy hash"
                      >
                        <Copy className="w-2.5 h-2.5" />
                      </button>
                    </button>
                    {selectedCommit === c.hash && commitDetail && (
                      <pre className="mx-3 mb-1 px-2 py-1.5 bg-zinc-950 rounded border border-zinc-800 text-[9px] text-zinc-500 whitespace-pre-wrap max-h-32 overflow-y-auto font-mono">
                        {commitDetail}
                      </pre>
                    )}
                  </div>
                ))
              )}
            </div>
          )}
        </div>

        {/* Version info */}
        {versionInfo && (
          <div className="px-3 py-2 border-b border-zinc-800/50">
            <div className="flex items-center gap-2">
              <Tag className="w-3.5 h-3.5 text-zinc-500" />
              <span className="text-[11px] text-zinc-400">
                Version: <span className="text-zinc-200 font-medium">{versionInfo.current}</span>
              </span>
              <span className="text-[10px] text-zinc-600 ml-auto">
                {versionInfo.total_commits} commits
              </span>
            </div>
          </div>
        )}

        {/* Publish section */}
        {gitStatus.is_repo && (
          <div className="px-3 py-3">
            <div className="space-y-2.5">
              {/* Commit message */}
              <input
                type="text"
                value={publishMessage}
                onChange={(e) => setPublishMessage(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && !publishing && totalChanges > 0) publish();
                }}
                placeholder="Describe your changes..."
                className="w-full px-2.5 py-2 bg-zinc-800 border border-zinc-700 rounded-lg text-sm text-zinc-100 placeholder:text-zinc-600 outline-none focus:border-blue-500 transition-colors"
              />

              {/* Version controls */}
              <div className="space-y-1.5">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={autoVersion}
                    onChange={(e) => setAutoVersion(e.target.checked)}
                    className="w-3.5 h-3.5 rounded border-zinc-600 bg-zinc-800 text-blue-500 focus:ring-blue-500 focus:ring-offset-0"
                  />
                  <span className="text-[11px] text-zinc-400">Tag version</span>
                </label>
                {autoVersion && versionInfo && (
                  <div className="flex gap-1 flex-wrap">
                    {([
                      { key: 'patch' as const, label: versionInfo.next_patch },
                      { key: 'minor' as const, label: versionInfo.next_minor },
                      { key: 'major' as const, label: versionInfo.next_major },
                      { key: 'custom' as const, label: 'Custom' },
                    ]).map(opt => (
                      <button
                        key={opt.key}
                        onClick={() => setVersionBump(opt.key)}
                        className={`px-2 py-0.5 rounded text-[10px] font-medium border transition-colors ${
                          versionBump === opt.key
                            ? 'border-blue-600 bg-blue-900/30 text-blue-300'
                            : 'border-zinc-700 bg-zinc-800 text-zinc-500 hover:text-zinc-300'
                        }`}
                      >
                        {opt.label}
                      </button>
                    ))}
                    {versionBump === 'custom' && (
                      <input
                        type="text"
                        value={customVersion}
                        onChange={(e) => setCustomVersion(e.target.value)}
                        placeholder="v1.0.0"
                        className="w-20 bg-zinc-900 border border-zinc-600 rounded px-1.5 py-0.5 text-[10px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-500"
                      />
                    )}
                  </div>
                )}
              </div>

              {/* Push target: repo + branch */}
              {gitStatus.has_remote && totalChanges > 0 && (
                <div className="space-y-1">
                  <div className="flex items-center gap-1.5 px-2 py-1.5 bg-zinc-800/60 rounded text-[10px] text-zinc-500">
                    <Upload className="w-3 h-3 shrink-0" />
                    <span>Push to</span>
                    <span className="text-zinc-300 font-medium truncate">
                      {gitStatus.remote_url.replace(/^https?:\/\/github\.com\//, '').replace(/\.git$/, '')}
                    </span>
                    <span className="text-zinc-600">·</span>
                    <button
                      onClick={() => setShowPushBranchPicker(!showPushBranchPicker)}
                      className="text-blue-400 hover:text-blue-300 flex items-center gap-0.5 transition-colors"
                    >
                      {pushTargetBranch || gitStatus.branch || 'main'}
                      <ChevronDown className="w-2.5 h-2.5" />
                    </button>
                  </div>
                  {showPushBranchPicker && branchInfo && (
                    <div className="bg-zinc-800 border border-zinc-700 rounded-lg overflow-hidden max-h-36 overflow-y-auto">
                      {/* Current local branch (default) */}
                      <button
                        onClick={() => { setPushTargetBranch(null); setShowPushBranchPicker(false); }}
                        className={`w-full flex items-center gap-2 px-3 py-1.5 text-[11px] hover:bg-zinc-700 transition-colors text-left ${
                          !pushTargetBranch ? 'text-blue-300 bg-blue-900/20' : 'text-zinc-300'
                        }`}
                      >
                        <GitBranch className="w-3 h-3 shrink-0" />
                        {gitStatus.branch} <span className="text-zinc-500">(current)</span>
                        {!pushTargetBranch && <CheckCircle2 className="w-3 h-3 text-blue-400 ml-auto" />}
                      </button>
                      {/* Other local branches */}
                      {branchInfo.branches.filter(b => b !== gitStatus.branch).map(b => (
                        <button
                          key={`push-local-${b}`}
                          onClick={() => { setPushTargetBranch(b); setShowPushBranchPicker(false); }}
                          className={`w-full flex items-center gap-2 px-3 py-1.5 text-[11px] hover:bg-zinc-700 transition-colors text-left ${
                            pushTargetBranch === b ? 'text-blue-300 bg-blue-900/20' : 'text-zinc-300'
                          }`}
                        >
                          <GitBranch className="w-3 h-3 shrink-0" />
                          {b}
                          {pushTargetBranch === b && <CheckCircle2 className="w-3 h-3 text-blue-400 ml-auto" />}
                        </button>
                      ))}
                      {/* Remote branches */}
                      {branchInfo.remote_branches.length > 0 && (
                        <div className="border-t border-zinc-700/50">
                          <div className="px-2.5 pt-1.5 pb-0.5">
                            <span className="text-[9px] text-zinc-500 uppercase tracking-wider font-semibold">Remote</span>
                          </div>
                          {branchInfo.remote_branches.map(b => (
                            <button
                              key={`push-remote-${b}`}
                              onClick={() => { setPushTargetBranch(b); setShowPushBranchPicker(false); }}
                              className={`w-full flex items-center gap-2 px-3 py-1.5 text-[11px] hover:bg-zinc-700 transition-colors text-left ${
                                pushTargetBranch === b ? 'text-blue-300 bg-blue-900/20' : 'text-zinc-400'
                              }`}
                            >
                              <Globe className="w-3 h-3 shrink-0 text-zinc-600" />
                              {b}
                              {pushTargetBranch === b && <CheckCircle2 className="w-3 h-3 text-blue-400 ml-auto" />}
                            </button>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}

              {/* Publish button */}
              <button
                onClick={publish}
                disabled={publishing || totalChanges === 0}
                className={`w-full flex flex-col items-center justify-center gap-1 px-4 py-2.5 rounded-lg text-sm font-medium transition-all ${
                  totalChanges > 0
                    ? 'bg-green-600 hover:bg-green-500 text-white shadow-lg shadow-green-900/30'
                    : 'bg-zinc-800 text-zinc-600 cursor-not-allowed'
                }`}
              >
                <div className="flex items-center gap-2">
                  {publishing ? (
                    <><Loader2 className="w-4 h-4 animate-spin" /> Publishing...</>
                  ) : totalChanges > 0 ? (
                    <><Upload className="w-4 h-4" /> Publish to GitHub</>
                  ) : (
                    <><CheckCircle2 className="w-4 h-4" /> Everything up to date</>
                  )}
                </div>
              </button>

              {/* Amend last commit */}
              {gitStatus.last_commit_message && (
                <div className="pt-1">
                  <button
                    onClick={() => { setShowAmend(!showAmend); if (!showAmend) setAmendMessage(gitStatus.last_commit_message); }}
                    className="text-[10px] text-zinc-600 hover:text-zinc-400 transition-colors flex items-center gap-1"
                  >
                    <GitCommitHorizontal className="w-3 h-3" />
                    Amend last commit
                  </button>
                  {showAmend && (
                    <div className="mt-1.5 space-y-1.5">
                      <input
                        type="text"
                        value={amendMessage}
                        onChange={(e) => setAmendMessage(e.target.value)}
                        onKeyDown={(e) => { if (e.key === 'Enter') amendCommit(); }}
                        placeholder="New commit message..."
                        className="w-full bg-zinc-900 border border-zinc-600 rounded px-2 py-1.5 text-[11px] text-zinc-200 placeholder:text-zinc-600 outline-none focus:border-blue-500"
                      />
                      <div className="flex gap-1.5">
                        <button
                          onClick={amendCommit}
                          className="flex-1 px-2 py-1 bg-amber-700 hover:bg-amber-600 rounded text-[11px] text-white font-medium transition-colors"
                        >
                          Amend
                        </button>
                        <button
                          onClick={() => { setShowAmend(false); setAmendMessage(''); }}
                          className="px-2 py-1 bg-zinc-800 hover:bg-zinc-700 rounded text-[11px] text-zinc-400 transition-colors"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      <StatusToast message={statusMessage} />
    </div>
  );
}

// ── Sub-components ──

function Header({ onRefresh }: { onRefresh?: () => void }) {
  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-zinc-800">
      <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
        Git & GitHub
      </span>
      {onRefresh && (
        <button
          onClick={onRefresh}
          className="p-1 rounded hover:bg-zinc-800 text-zinc-500 hover:text-zinc-300 transition-colors"
          title="Refresh"
        >
          <RefreshCw className="w-3.5 h-3.5" />
        </button>
      )}
    </div>
  );
}

function SetupStep({
  number,
  label,
  done,
  active,
  children,
}: {
  number: number;
  label: string;
  done: boolean;
  active: boolean;
  children?: React.ReactNode;
}) {
  return (
    <div className={`${active ? '' : 'opacity-60'}`}>
      <div className="flex items-center gap-2">
        {done ? (
          <CheckCircle2 className="w-4 h-4 text-green-500 shrink-0" />
        ) : active ? (
          <AlertCircle className="w-4 h-4 text-blue-400 shrink-0" />
        ) : (
          <div className="w-4 h-4 rounded-full border border-zinc-600 flex items-center justify-center shrink-0">
            <span className="text-[9px] text-zinc-500">{number}</span>
          </div>
        )}
        <span className={`text-xs ${done ? 'text-green-400' : active ? 'text-zinc-200' : 'text-zinc-500'}`}>
          {label}
          {done && ' ✓'}
        </span>
      </div>
      {children}
    </div>
  );
}

function FileRow({
  file,
  onStage,
  onDiscard,
  staged,
}: {
  file: ChangedFile;
  onStage: () => void;
  onDiscard: () => void;
  staged: boolean;
}) {
  const statusIcon = () => {
    switch (file.status) {
      case 'M': return <FileEdit className="w-3 h-3 text-yellow-400 shrink-0" />;
      case 'A': case '?': return <FilePlus className="w-3 h-3 text-green-400 shrink-0" />;
      case 'D': return <FileX className="w-3 h-3 text-red-400 shrink-0" />;
      case 'R': return <FileEdit className="w-3 h-3 text-blue-400 shrink-0" />;
      default: return <FileEdit className="w-3 h-3 text-zinc-500 shrink-0" />;
    }
  };

  const fileName = file.path.split('/').pop() || file.path;
  const dirPath = file.path.includes('/') ? file.path.substring(0, file.path.lastIndexOf('/')) : '';

  return (
    <div className="flex items-center gap-1.5 px-3 py-0.5 group hover:bg-zinc-800/40 transition-colors">
      {statusIcon()}
      <span className="text-[11px] text-zinc-300 truncate flex-1" title={file.path}>
        {fileName}
        {dirPath && <span className="text-zinc-600 ml-1">{dirPath}</span>}
      </span>
      <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <button
          onClick={onStage}
          className="p-0.5 text-zinc-600 hover:text-blue-400 transition-colors"
          title={staged ? 'Unstage' : 'Stage'}
        >
          {staged ? <MinusCircle className="w-3 h-3" /> : <PlusCircle className="w-3 h-3" />}
        </button>
        {!staged && (
          <button
            onClick={onDiscard}
            className="p-0.5 text-zinc-600 hover:text-red-400 transition-colors"
            title="Discard changes"
          >
            <Undo2 className="w-3 h-3" />
          </button>
        )}
      </div>
    </div>
  );
}

function StatusToast({ message }: { message: { type: 'success' | 'error'; text: string } | null }) {
  if (!message) return null;
  return (
    <div className={`mx-3 mb-3 px-3 py-2 rounded-lg text-xs ${
      message.type === 'success'
        ? 'bg-green-900/30 text-green-300 border border-green-800/50'
        : 'bg-red-900/30 text-red-300 border border-red-800/50'
    }`}>
      <div className="flex items-start gap-2">
        {message.type === 'success' ? (
          <CheckCircle2 className="w-3.5 h-3.5 mt-0.5 shrink-0" />
        ) : (
          <XCircle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
        )}
        <span className="break-words">{message.text}</span>
      </div>
    </div>
  );
}
