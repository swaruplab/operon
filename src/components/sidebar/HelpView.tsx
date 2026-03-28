import { useState } from 'react';
import {
  HelpCircle,
  ChevronRight,
  ChevronDown,
  Bot,
  ClipboardList,
  MessageSquare,
  Terminal,
  FolderTree,
  Code2,
  Server,
  Keyboard,
  BookOpen,
  Zap,
  PlayCircle,
  Search,
  Sparkles,
  GitBranch,
  Mic,
  BookMarked,
  Settings2,
  Plug,
  Puzzle,
} from 'lucide-react';

interface HelpSection {
  id: string;
  title: string;
  icon: React.ElementType;
  iconColor: string;
  items: HelpItem[];
}

interface HelpItem {
  title: string;
  content: string;
  tip?: string;
  action?: { label: string; handler: () => void };
  shortcut?: string;
}

interface HelpViewProps {
  onViewChange?: (view: string) => void;
}

export function HelpView({ onViewChange }: HelpViewProps) {
  const [expandedSection, setExpandedSection] = useState<string | null>('getting-started');
  const [expandedItem, setExpandedItem] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState('');

  const toggleSection = (id: string) => {
    setExpandedSection(expandedSection === id ? null : id);
    setExpandedItem(null);
  };

  const toggleItem = (id: string) => {
    setExpandedItem(expandedItem === id ? null : id);
  };

  const sections: HelpSection[] = [
    {
      id: 'getting-started',
      title: 'Getting Started',
      icon: Sparkles,
      iconColor: 'text-blue-400',
      items: [
        {
          title: 'Opening a project',
          content: 'Click "Open Folder" in the file explorer sidebar, or drag a folder onto the app window. Operon will set this as your working directory — Claude will be able to read, edit, and create files within it.',
          action: onViewChange ? { label: 'Open Explorer', handler: () => onViewChange('files') } : undefined,
        },
        {
          title: 'Your first prompt',
          content: 'Type a message in the chat panel on the right. Try something like "What does this project do?" or "Help me fix the bug in main.py". Claude will read your files, understand the context, and respond.',
          tip: 'Start with Ask mode to explore your codebase before making changes.',
        },
        {
          title: 'Understanding the layout',
          content: 'Operon has four main areas: the Activity Bar (far left icons), the Sidebar (file explorer, SSH, etc.), the Editor (center, for code), and the Chat Panel (right side). The Terminal lives in a bottom panel you can toggle. All panels are resizable by dragging their borders.',
        },
        {
          title: 'Relaunch setup wizard',
          content: 'If you need to reconfigure authentication or review the onboarding tour, you can relaunch the setup wizard from settings.',
          action: {
            label: 'Open Settings',
            handler: () => onViewChange?.('settings'),
          },
        },
      ],
    },
    {
      id: 'ai-modes',
      title: 'AI Modes',
      icon: Bot,
      iconColor: 'text-purple-400',
      items: [
        {
          title: 'Agent Mode',
          content: 'The default and most powerful mode. Claude can read files, write code, run terminal commands, and make multi-step changes to your project autonomously. Use this for implementing features, fixing bugs, refactoring, running pipelines, and any task where you want Claude to act.',
          tip: 'Claude shows each file it reads and edits in real-time. You can stop it at any time with the stop button.',
        },
        {
          title: 'Plan Mode',
          content: 'Claude creates a detailed implementation plan (saved as implementation_plan.md) before writing any code. You can review the plan in the editor, give feedback to refine it, and approve it when ready. Claude then executes the plan step by step.',
          tip: 'Use quick feedback buttons or type your own feedback in the chat to iterate on the plan before approving.',
        },
        {
          title: 'Ask Mode',
          content: 'Claude answers questions and explains code without making any changes to your project. Use this to understand how code works, get explanations of error messages, learn about libraries, or discuss architecture decisions.',
          tip: 'Great for onboarding onto a new codebase — ask "Walk me through how the authentication flow works".',
        },
        {
          title: 'Switching modes',
          content: 'Click the mode selector above the chat input to switch between Agent, Plan, and Ask modes. You can switch modes mid-conversation. The mode affects what Claude is allowed to do — it doesn\'t lose context when you switch.',
          shortcut: 'Click the mode selector above the chat input',
        },
      ],
    },
    {
      id: 'pubmed',
      title: 'PubMed Literature',
      icon: BookMarked,
      iconColor: 'text-emerald-400',
      items: [
        {
          title: 'What is PubMed grounding?',
          content: 'When enabled in Ask mode, Operon automatically searches PubMed for peer-reviewed articles relevant to your question before Claude responds. Claude then grounds its answer in real scientific literature, citing specific papers with links you can follow.',
          tip: 'This is especially powerful for questions about genes, pathways, methods, or any topic covered in biomedical literature.',
        },
        {
          title: 'Enabling PubMed search',
          content: 'Switch to Ask mode using the mode selector above the chat input. You\'ll see a green "PubMed" toggle button appear. Click it to enable or disable literature search. When enabled, every question you ask will first search PubMed, then Claude answers using those papers as context.',
          tip: 'The PubMed toggle only appears in Ask mode — it\'s not available in Agent or Plan modes.',
        },
        {
          title: 'How it works',
          content: 'Operon extracts key scientific terms from your question, queries the NCBI PubMed E-utilities API, and retrieves up to 5 relevant articles with full abstracts. These are injected into the prompt so Claude can cite them by number [1], [2], etc. Each response includes a References section with PubMed links.',
        },
        {
          title: 'Reading the results',
          content: 'After a PubMed-grounded response, a green bar appears above the chat input showing the articles that were found. Click it to expand and see titles, authors, journals, and direct PubMed links. Click any PMID link to open the paper on PubMed.',
        },
        {
          title: 'Tips for better results',
          content: 'Use specific scientific terms for better PubMed matches. For example, "What is the role of TP53 in apoptosis?" will yield better results than "how does cell death work?". Gene names, pathway names, method names, and disease terms all work well as search queries.',
        },
      ],
    },
    {
      id: 'voice',
      title: 'Voice Dictation',
      icon: Mic,
      iconColor: 'text-red-400',
      items: [
        {
          title: 'Using voice input',
          content: 'Click the microphone icon next to the send button in the chat input area. Operon uses macOS native speech recognition (SFSpeechRecognizer) to convert your speech to text in real-time. Click the mic again to stop recording.',
          tip: 'The mic button pulses red while actively listening. Your words appear in the text input as you speak.',
        },
        {
          title: 'First-time setup',
          content: 'The first time you use voice dictation, macOS will prompt you to grant two permissions: Microphone access and Speech Recognition access. Both must be allowed for dictation to work. You can manage these in System Settings → Privacy & Security.',
        },
        {
          title: 'How it works',
          content: 'Operon launches a native macOS speech recognition process using Apple\'s SFSpeechRecognizer framework. Your audio is processed locally or via Apple\'s servers (depending on your macOS settings). Partial results stream into the text field as you speak, and the final transcription replaces them when you stop.',
        },
        {
          title: 'Tips for best results',
          content: 'Speak clearly at a natural pace. Technical terms and gene names may need correction after dictation — review the transcription before sending. You can edit the transcribed text just like any other text in the input field. Dictation works best in quiet environments.',
        },
      ],
    },
    {
      id: 'github',
      title: 'GitHub Integration',
      icon: GitBranch,
      iconColor: 'text-orange-400',
      items: [
        {
          title: 'Overview',
          content: 'Operon includes built-in GitHub integration for version control and publishing. You can initialize a repo, commit changes, and publish your project to GitHub — all from the Git panel in the sidebar without ever opening a terminal.',
          action: onViewChange ? { label: 'Open Git Panel', handler: () => onViewChange('git') } : undefined,
        },
        {
          title: 'Setting up GitHub',
          content: 'Open the Git panel from the sidebar (the branch icon). The first time, Operon will guide you through a 3-step setup: 1) Install the GitHub CLI (gh) if not present, 2) Sign in to your GitHub account using device authentication, 3) Create a new repository on GitHub for your project.',
        },
        {
          title: 'GitHub sign-in',
          content: 'Operon uses GitHub\'s secure device authentication flow. When you click "Sign in to GitHub", a one-time code appears. Copy it, then click the link to open GitHub in your browser. Paste the code to authorize Operon. Once authenticated, you stay signed in across sessions.',
          tip: 'The one-time code is displayed in the Git panel with a click-to-copy button for convenience.',
        },
        {
          title: 'Committing and publishing',
          content: 'Once set up, the Git panel shows your current branch, changed files count, and version info. Write a commit message and click "Commit & Push" to save your work. The "Publish" button handles creating a version tag and pushing everything to GitHub in one click.',
        },
        {
          title: 'Auto-versioning',
          content: 'Enable "Auto Version" in the Git panel to automatically bump the patch version (e.g., 0.1.0 → 0.1.1) each time you publish. Operon uses semantic versioning (semver) and creates git tags for each release.',
        },
      ],
    },
    {
      id: 'editor',
      title: 'Code Editor',
      icon: Code2,
      iconColor: 'text-green-400',
      items: [
        {
          title: 'Opening files',
          content: 'Click any file in the sidebar explorer to open it in the editor. Files open as tabs — click a tab to switch, or middle-click to close. Double-click a file to pin it (single-click opens as a preview that gets replaced by the next file you open).',
        },
        {
          title: 'Editing and saving',
          content: 'Edit files directly in the Monaco editor. Changes are indicated by a blue dot on the tab. Save with Cmd+S. The editor supports syntax highlighting for 50+ languages, bracket matching, auto-indent, and multi-cursor editing.',
          shortcut: 'Cmd+S to save',
        },
        {
          title: 'Diff view',
          content: 'When Claude edits a file, a diff view shows what changed (green = added, red = removed). You can review changes before they\'re applied. Click "Accept" to keep changes or "Reject" to revert.',
        },
        {
          title: 'Previewing files',
          content: 'Image files (PNG, JPG, SVG, etc.) open in a visual viewer with zoom and rotation. PDFs render inline. HTML files show a live preview. These all open as tabs alongside your code files.',
        },
      ],
    },
    {
      id: 'terminal',
      title: 'Terminal',
      icon: Terminal,
      iconColor: 'text-amber-400',
      items: [
        {
          title: 'Using the terminal',
          content: 'The integrated terminal runs in the bottom panel. It\'s a full shell (zsh/bash) connected to your project directory. You can run any command — build tools, git, scripts, package managers, etc.',
        },
        {
          title: 'Claude and the terminal',
          content: 'In Agent mode, Claude can run terminal commands autonomously. You\'ll see commands and their output in the chat. Claude uses the terminal to install dependencies, run tests, execute scripts, and more.',
        },
        {
          title: 'Multiple terminals',
          content: 'When you connect to a remote server via SSH, a second terminal tab appears for the remote session. You can have both local and remote terminals active simultaneously.',
        },
      ],
    },
    {
      id: 'remote-ssh',
      title: 'Remote SSH & HPC',
      icon: Server,
      iconColor: 'text-teal-400',
      items: [
        {
          title: 'Adding a server',
          content: 'Go to the SSH view in the sidebar and click "Add Server". Enter your hostname, username, and either an SSH key path or password. Operon stores profiles locally and can generate SSH keys for you automatically.',
          action: onViewChange ? { label: 'Open SSH View', handler: () => onViewChange('ssh') } : undefined,
        },
        {
          title: 'Connecting',
          content: 'Click "Connect" on a saved profile. This opens an SSH terminal in the bottom panel and switches the file explorer to show the remote filesystem. You can browse, open, and edit remote files.',
        },
        {
          title: 'Running Claude remotely',
          content: 'Select "Remote" next to the mode selector in the chat panel, then pick your connected server. Claude runs inside a tmux session on the remote machine, so sessions persist even if you disconnect or close the app.',
          tip: 'Perfect for long-running bioinformatics pipelines on HPC clusters — start a job and check back later.',
        },
        {
          title: 'SSH key setup',
          content: 'Don\'t have an SSH key? Expand the "Generate one automatically" section when adding a server. Enter your server password once — Operon generates an ed25519 key, copies it to the server, and stores it locally. You\'ll never need the password again.',
        },
        {
          title: 'HPC tips',
          content: 'Claude can submit Slurm/PBS jobs, check queue status, parse log files, and process results on your cluster. Try: "Submit a STAR alignment job for the samples in /data/fastq/" or "Check the status of my running jobs".',
        },
      ],
    },
    {
      id: 'server-config',
      title: 'Server Configuration',
      icon: Settings2,
      iconColor: 'text-cyan-400',
      items: [
        {
          title: 'What is Server Configuration?',
          content: 'Save HPC-specific settings — SLURM account, partitions, conda envs, paths — on each SSH profile. These are automatically injected into every protocol and AI-generated script for that server.',
        },
        {
          title: 'Setting up',
          content: 'Double-click an SSH profile to edit it. Expand "Server Configuration" and fill in the fields. Settings are saved with the profile and used everywhere.',
          action: onViewChange ? { label: 'Open SSH View', handler: () => onViewChange('ssh') } : undefined,
        },
        {
          title: 'Available fields',
          content: 'SLURM Account, CPU/GPU Partition, GPU Type, Default Conda Env, Modules, Scratch Directory, Working Directory — plus custom key-value pairs for anything specific to your setup.',
        },
        {
          title: 'How it works with AI',
          content: 'When connected to a server, your config is automatically included in every prompt. Say "submit a STAR job" and Claude already knows your SLURM account, partition, and paths.',
        },
        {
          title: 'Custom variables',
          content: 'Add any key-value pair via "+ Add custom variable" — useful for PI names, project codes, shared data paths, or anything you reference frequently.',
        },
      ],
    },
    {
      id: 'protocols',
      title: 'Protocols',
      icon: BookOpen,
      iconColor: 'text-indigo-400',
      items: [
        {
          title: 'What are protocols?',
          content: 'Protocols are reusable prompt templates for common workflows. Each protocol is a folder with a PROTOCOL.md entry point plus optional reference files, scripts, and templates. When activated, the protocol context is included with every message to Claude.',
          action: onViewChange ? { label: 'View Protocols', handler: () => onViewChange('protocols') } : undefined,
        },
        {
          title: 'Creating a protocol',
          content: 'Create a folder in ~/.operon/protocols/ with a PROTOCOL.md file. This markdown file should describe the workflow, expected inputs, and how Claude should handle each step. You can include sub-folders with reference docs, example configs, or script templates.',
        },
        {
          title: 'Example use cases',
          content: 'Protocols are great for standardized workflows: RNA-seq analysis pipelines, variant calling procedures, quality control checklists, paper writing templates, or lab notebook formatting. Any workflow you repeat regularly can become a protocol.',
        },
      ],
    },
    {
      id: 'mcp-servers',
      title: 'MCP Servers',
      icon: Plug,
      iconColor: 'text-rose-400',
      items: [
        {
          title: 'What are MCP servers?',
          content: 'MCP (Model Context Protocol) servers are plugins that give Claude access to external tools and databases during chat. When enabled, Claude can automatically call tools to search databases, fetch data, and perform analyses within the conversation.',
          tip: 'Think of MCP servers as superpowers for Claude — with them, Claude can query ENCODE, PubMed, protein databases, and more.',
        },
        {
          title: 'Built-in research catalog',
          content: 'Operon ships with curated research MCP servers:\n\n• ENCODE Toolkit — 14 genomics databases (ENCODE, GTEx, ClinVar, GWAS Catalog, JASPAR, CellxGene, gnomAD, Ensembl, UCSC, GEO, PubMed, bioRxiv, ClinicalTrials.gov, Open Targets) with 20 tools.\n\n• BioMCP — Protein structure analysis via PDB.',
        },
        {
          title: 'Enabling an MCP server',
          content: 'Go to Settings → MCP Servers. Toggle a server on in the Research Tools Catalog. Operon checks your runtime (Python or Node.js), installs the package, and configures it. Available in your next chat session.',
          action: onViewChange ? { label: 'Open Settings', handler: () => onViewChange('settings') } : undefined,
        },
        {
          title: 'Using MCP tools in chat',
          content: 'Just ask naturally — Claude calls the right tools automatically:\n\n• "Search ENCODE for ATAC-seq in human brain"\n• "Find ClinVar variants for BRCA1"\n• "Analyze the active site of PDB 1A2B"\n\nTool calls show as labeled badges with expandable input/output.',
        },
        {
          title: 'Adding custom servers',
          content: 'In Settings → MCP Servers → Custom Servers, enter a name, command, and args for any MCP-compatible server.',
          tip: 'Any server that speaks MCP over stdio will work.',
        },
        {
          title: 'Remote MCP servers',
          content: 'MCP servers work on remote machines too. When running Claude on your HPC cluster via SSH, Operon writes the MCP config remotely so tools like ENCODE Toolkit are available there.',
        },
        {
          title: 'Runtime requirements',
          content: 'ENCODE Toolkit needs Python 3.10+ (pip install encode-toolkit). BioMCP needs Node.js 20+ (npm install -g @anthropic-ai/bio-mcp). Operon checks these before enabling.',
        },
      ],
    },
    {
      id: 'extensions',
      title: 'Extensions',
      icon: Puzzle,
      iconColor: 'text-violet-400',
      items: [
        {
          title: 'What are extensions?',
          content: 'Extensions add language support, themes, snippets, and code intelligence to the editor. Operon uses the Open VSX registry — thousands of extensions for syntax highlighting, LSP, and more.',
        },
        {
          title: 'Browsing and installing',
          content: 'Open Extensions from the Activity Bar (puzzle icon). Search or browse by category. Click "Install" to download and activate. Extensions persist in ~/.config/operon/extensions/.',
        },
        {
          title: 'Language servers (LSP)',
          content: 'Many extensions include a language server for autocompletion, hover docs, diagnostics, and go-to-definition. Operon auto-starts the matching server when you open a file.',
          tip: 'If no LSP is installed for a language, Operon recommends an extension.',
        },
        {
          title: 'Themes and snippets',
          content: 'Install theme extensions and select them in Settings. Snippets activate automatically — type a prefix in the editor to see completions.',
        },
        {
          title: 'Extension settings',
          content: 'Per-extension settings are in Settings → Extensions. These are parsed from the extension\'s package.json and rendered as interactive forms.',
          action: onViewChange ? { label: 'Open Settings', handler: () => onViewChange('settings') } : undefined,
        },
        {
          title: 'Sideloading a VSIX',
          content: 'Have a .vsix file? Use the sideload option in the Extensions view to install directly without the registry.',
        },
        {
          title: 'Remote extensions',
          content: 'When working via SSH, Operon can install extensions and run language servers on the remote machine for full code intelligence on remote projects.',
        },
        {
          title: 'Docker & Singularity tools',
          content: 'Built-in tool extensions for container management:\n\n• Docker — List, start, stop, remove containers. View images/volumes. Read logs.\n\n• Singularity/Apptainer — Manage .sif images and instances for HPC.',
        },
      ],
    },
    {
      id: 'shortcuts',
      title: 'Keyboard Shortcuts',
      icon: Keyboard,
      iconColor: 'text-zinc-400',
      items: [
        {
          title: 'Chat shortcuts',
          content: 'Cmd+K: New conversation\nCmd+L: Focus chat input\nEnter: Send message\nShift+Enter: New line in message\nEsc: Stop Claude\'s response',
        },
        {
          title: 'Editor shortcuts',
          content: 'Cmd+S: Save file\nCmd+W: Close tab\nCmd+P: Quick open file\nCmd+Shift+P: Command palette\nCmd+/: Toggle comment\nCmd+D: Select next occurrence',
        },
        {
          title: 'Navigation',
          content: 'Cmd+1: Focus sidebar\nCmd+B: Toggle sidebar\nCmd+J: Toggle terminal\nCmd+\\: Focus editor\nCmd+Shift+E: Explorer view',
        },
      ],
    },
    {
      id: 'tips',
      title: 'Tips & Best Practices',
      icon: Zap,
      iconColor: 'text-yellow-400',
      items: [
        {
          title: 'Be specific with Claude',
          content: 'Instead of "fix this", try "fix the TypeError on line 45 of parser.py — it\'s failing when the input file has empty lines". The more context you provide, the better Claude\'s response.',
        },
        {
          title: 'Use Plan mode for complex tasks',
          content: 'For multi-file changes, pipeline setups, or architectural decisions, switch to Plan mode first. Review the plan, give feedback, and iterate before Claude writes any code. This prevents wasted effort on the wrong approach.',
        },
        {
          title: 'Reference files with @',
          content: 'Type @ in the chat input to reference specific files. This helps Claude focus on exactly the files you care about instead of searching the entire project.',
        },
        {
          title: 'Break big tasks into steps',
          content: 'Instead of "build me a complete analysis pipeline", start with "set up the project structure and config for a Nextflow RNA-seq pipeline", then iterate from there. Claude works best with focused, incremental tasks.',
        },
        {
          title: 'Check Claude\'s work',
          content: 'Always review diffs before accepting changes. Use Ask mode to have Claude explain its approach. Run tests after Claude makes changes. Claude is powerful but not infallible — treat it as a very capable collaborator, not an oracle.',
        },
      ],
    },
  ];

  // Filter sections by search
  const filteredSections = searchQuery.trim()
    ? sections.map(section => ({
        ...section,
        items: section.items.filter(item =>
          item.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
          item.content.toLowerCase().includes(searchQuery.toLowerCase())
        ),
      })).filter(section => section.items.length > 0)
    : sections;

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-zinc-800 shrink-0">
        <HelpCircle className="w-3.5 h-3.5 text-zinc-500" />
        <span className="text-[11px] font-semibold text-zinc-500 uppercase tracking-wider">
          Help
        </span>
      </div>

      {/* Search */}
      <div className="px-3 py-2 border-b border-zinc-800 shrink-0">
        <div className="relative">
          <Search className="absolute left-2 top-1/2 -translate-y-1/2 w-3 h-3 text-zinc-600" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search help topics..."
            className="w-full bg-zinc-800 border border-zinc-700 rounded pl-7 pr-3 py-1 text-xs text-zinc-200 outline-none focus:border-zinc-600 placeholder:text-zinc-600"
            spellCheck={false}
          />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto">
        {filteredSections.map((section) => (
          <div key={section.id}>
            {/* Section header */}
            <button
              onClick={() => toggleSection(section.id)}
              className="w-full flex items-center gap-2 px-3 py-2 hover:bg-zinc-800/50 transition-colors text-left"
            >
              {expandedSection === section.id ? (
                <ChevronDown className="w-3 h-3 text-zinc-500 shrink-0" />
              ) : (
                <ChevronRight className="w-3 h-3 text-zinc-500 shrink-0" />
              )}
              <section.icon className={`w-3.5 h-3.5 ${section.iconColor} shrink-0`} />
              <span className="text-xs font-medium text-zinc-300">{section.title}</span>
              <span className="text-[10px] text-zinc-600 ml-auto">{section.items.length}</span>
            </button>

            {/* Section items */}
            {expandedSection === section.id && (
              <div className="pb-1">
                {section.items.map((item, idx) => {
                  const itemId = `${section.id}-${idx}`;
                  const isExpanded = expandedItem === itemId;

                  return (
                    <div key={itemId}>
                      <button
                        onClick={() => toggleItem(itemId)}
                        className={`w-full flex items-center gap-2 px-3 pl-9 py-1.5 hover:bg-zinc-800/30 transition-colors text-left ${
                          isExpanded ? 'bg-zinc-800/20' : ''
                        }`}
                      >
                        <span className={`text-[11px] ${isExpanded ? 'text-zinc-200' : 'text-zinc-400'}`}>
                          {item.title}
                        </span>
                        {item.shortcut && (
                          <kbd className="text-[9px] bg-zinc-800 px-1 py-0.5 rounded text-zinc-500 font-mono ml-auto shrink-0">
                            {item.shortcut}
                          </kbd>
                        )}
                      </button>

                      {isExpanded && (
                        <div className="px-3 pl-9 pr-4 pb-2 space-y-2">
                          <p className="text-[11px] text-zinc-500 leading-relaxed whitespace-pre-line">
                            {item.content}
                          </p>

                          {item.tip && (
                            <div className="flex gap-2 p-2 bg-blue-950/20 rounded border border-blue-900/20">
                              <Zap className="w-3 h-3 text-blue-400 shrink-0 mt-0.5" />
                              <p className="text-[10px] text-blue-300/80 leading-relaxed">{item.tip}</p>
                            </div>
                          )}

                          {item.action && (
                            <button
                              onClick={item.action.handler}
                              className="inline-flex items-center gap-1.5 px-2.5 py-1 bg-zinc-800 hover:bg-zinc-700 text-[10px] text-zinc-300 rounded transition-colors"
                            >
                              <PlayCircle className="w-3 h-3 text-blue-400" />
                              {item.action.label}
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        ))}

        {filteredSections.length === 0 && searchQuery && (
          <div className="flex flex-col items-center justify-center py-8 text-center">
            <Search className="w-5 h-5 text-zinc-700 mb-2" />
            <p className="text-xs text-zinc-600">No results for "{searchQuery}"</p>
            <button
              onClick={() => setSearchQuery('')}
              className="text-[10px] text-blue-400 hover:text-blue-300 mt-1"
            >
              Clear search
            </button>
          </div>
        )}

        {/* Footer */}
        <div className="px-3 py-3 mt-2 border-t border-zinc-800/50">
          <p className="text-[10px] text-zinc-600 leading-relaxed">
            Operon is powered by Claude Code from Anthropic.
            For more about Claude Code, visit{' '}
            <a
              href="https://docs.anthropic.com/en/docs/claude-code"
              target="_blank"
              rel="noopener noreferrer"
              className="text-blue-500 hover:text-blue-400"
            >
              docs.anthropic.com
            </a>
          </p>
        </div>
      </div>
    </div>
  );
}
