/* =========================================================
   Operon — shared site behaviour
   ========================================================= */

/* ---- Download config loader ---- */
async function loadDownloads() {
  let cfg = null;
  try {
    const r = await fetch('downloads.json', { cache: 'no-cache' }).catch(() => fetch('./downloads.json'));
    cfg = await r.json();
  } catch (e) {
    console.warn('Could not load downloads.json', e);
    return;
  }
  window.__OPERON__ = cfg;
  wireDownloads(cfg);

  // Try to overlay with the live GitHub release (non-blocking)
  syncFromGitHub(cfg).then(synced => {
    if (synced) wireDownloads(synced);
  });

  return cfg;
}

/* ---- Pull latest release from GitHub so the site never drifts ---- */
async function syncFromGitHub(base) {
  // Prefer the tauri-updater manifest (public, no auth, small payload)
  const manifestUrl = base?.updater_manifest || 'https://github.com/swaruplab/operon/releases/latest/download/latest.json';
  try {
    const r = await fetch(manifestUrl, { cache: 'no-cache' });
    if (!r.ok) throw new Error('status ' + r.status);
    const m = await r.json();
    if (!m?.version || !m?.platforms) return null;

    const v = m.version;
    const P = m.platforms;
    const url = (k) => P[k]?.url;

    // Derive filenames from URLs
    const fn = (u) => (u ? u.split('/').pop() : undefined);

    const merged = JSON.parse(JSON.stringify(base));
    merged.version = v;
    merged.pub_date = m.pub_date || merged.pub_date;

    const mac_arm = url('darwin-aarch64');
    const mac_x64 = url('darwin-x86_64');
    const win_nsis = url('windows-x86_64-nsis') || url('windows-x86_64');
    const win_msi = url('windows-x86_64-msi');
    const linux_deb = url('linux-x86_64-deb');
    const linux_appimage = url('linux-x86_64-appimage') || url('linux-x86_64');
    const linux_rpm = url('linux-x86_64-rpm');

    // macOS builds are served from Dropbox (signed .dmg) — do NOT overlay from
    // GitHub releases, which still publish the older .app.tar.gz artifacts.
    // Uncomment the two lines below once macOS .dmg builds are uploaded to GitHub Releases.
    // if (mac_arm)         { merged.downloads.apple_silicon.url = mac_arm;         merged.downloads.apple_silicon.filename = fn(mac_arm); }
    // if (mac_x64)         { merged.downloads.intel.url = mac_x64;                 merged.downloads.intel.filename = fn(mac_x64); }
    if (win_nsis)        { merged.downloads.windows.url = win_nsis;              merged.downloads.windows.filename = fn(win_nsis); }
    if (win_msi && merged.downloads.windows_msi)   { merged.downloads.windows_msi.url = win_msi;      merged.downloads.windows_msi.filename = fn(win_msi); }
    if (linux_deb)       { merged.downloads.linux_deb.url = linux_deb;           merged.downloads.linux_deb.filename = fn(linux_deb); }
    if (linux_appimage)  { merged.downloads.linux_appimage.url = linux_appimage; merged.downloads.linux_appimage.filename = fn(linux_appimage); }
    if (linux_rpm && merged.downloads.linux_rpm)   { merged.downloads.linux_rpm.url = linux_rpm;      merged.downloads.linux_rpm.filename = fn(linux_rpm); }

    window.__OPERON__ = merged;
    return merged;
  } catch (e) {
    // GitHub blocked, offline, CORS, or asset not published yet — just keep the static config
    return null;
  }
}

/* ---- Best-guess OS detection ---- */
function detectOS() {
  const ua = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent || '').toLowerCase();
  if (ua.includes('mac')) {
    return { os: 'macOS', arch: (ua.includes('arm') ? 'arm64' : 'detect') };
  }
  if (ua.includes('win')) return { os: 'Windows', arch: 'x64' };
  if (ua.includes('linux')) return { os: 'Linux', arch: 'x64' };
  return { os: 'macOS', arch: 'detect' };
}

/* ---- Wire download buttons on the page ---- */
function wireDownloads(cfg) {
  const v = cfg.version;
  const d = cfg.downloads;
  const os = detectOS();

  // Primary CTA with smart default
  document.querySelectorAll('[data-download="primary"]').forEach(btn => {
    let pick = d.apple_silicon;
    let label = 'Download for Apple Silicon';
    if (os.os === 'macOS') {
      pick = d.apple_silicon;
      label = 'Download for Mac';
    } else if (os.os === 'Windows') {
      pick = d.windows;
      label = 'Download for Windows';
    } else if (os.os === 'Linux') {
      pick = d.linux_deb;
      label = 'Download for Linux';
    }
    btn.href = pick.url;
    btn.setAttribute('download', pick.filename);
    const lbl = btn.querySelector('[data-download-label]');
    if (lbl) lbl.textContent = label;
  });

  // Version pills
  document.querySelectorAll('[data-version]').forEach(el => {
    el.textContent = 'v' + v;
  });
  document.querySelectorAll('[data-version-plain]').forEach(el => {
    el.textContent = v;
  });
  // Protocol count
  document.querySelectorAll('[data-protocol-count]').forEach(el => {
    el.textContent = cfg.protocol_count;
  });
  document.querySelectorAll('[data-mcp-count]').forEach(el => {
    el.textContent = cfg.mcp_count || '12+';
  });

  // Per-platform direct links
  const map = {
    'dl-apple-silicon': d.apple_silicon,
    'dl-intel': d.intel,
    'dl-windows': d.windows,
    'dl-windows-msi': d.windows_msi,
    'dl-linux-deb': d.linux_deb,
    'dl-linux-appimage': d.linux_appimage,
    'dl-linux-rpm': d.linux_rpm
  };
  for (const [id, cfgEntry] of Object.entries(map)) {
    document.querySelectorAll(`[data-dl="${id}"]`).forEach(a => {
      if (!cfgEntry) { a.style.display = 'none'; return; }
      a.href = cfgEntry.url;
      a.setAttribute('download', cfgEntry.filename);
      const fn = a.querySelector('[data-filename]');
      if (fn) fn.textContent = cfgEntry.filename;
    });
  }
}

/* ---- Download dropdown ---- */
function wireDownloadDropdown() {
  document.querySelectorAll('.download-menu').forEach(menu => {
    const trigger = menu.querySelector('[data-download-trigger]');
    if (!trigger) return;
    const row = menu.closest('.row');
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      e.preventDefault();
      menu.classList.toggle('open');
      if (row) row.classList.toggle('dropdown-open', menu.classList.contains('open'));
    });
  });
  const closeAll = () => {
    document.querySelectorAll('.download-menu.open').forEach(m => {
      m.classList.remove('open');
      const r = m.closest('.row');
      if (r) r.classList.remove('dropdown-open');
    });
  };
  document.addEventListener('click', closeAll);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') closeAll();
  });
}

/* ---- Nav behavior (scroll-shadow + mobile toggle) ---- */
function wireNav() {
  const nav = document.querySelector('.nav');
  if (!nav) return;
  const onScroll = () => {
    nav.classList.toggle('scrolled', window.scrollY > 8);
  };
  onScroll();
  window.addEventListener('scroll', onScroll, { passive: true });

  const toggle = nav.querySelector('.nav-toggle');
  if (toggle) {
    toggle.addEventListener('click', () => nav.classList.toggle('menu-open'));
  }
  nav.querySelectorAll('.nav-links a').forEach(a => {
    a.addEventListener('click', () => nav.classList.remove('menu-open'));
  });
}

/* ---- Scroll reveal ---- */
function wireReveal() {
  const els = document.querySelectorAll('.reveal');
  if (!('IntersectionObserver' in window)) {
    els.forEach(el => el.classList.add('is-visible'));
    return;
  }
  const obs = new IntersectionObserver((entries) => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        e.target.classList.add('is-visible');
        obs.unobserve(e.target);
      }
    });
  }, { rootMargin: '0px 0px -10% 0px', threshold: 0.05 });
  els.forEach(el => obs.observe(el));
}

/* ---- Back to top ---- */
function wireBackTop() {
  const btn = document.querySelector('.backtop');
  if (!btn) return;
  window.addEventListener('scroll', () => {
    btn.classList.toggle('show', window.scrollY > 800);
  }, { passive: true });
  btn.addEventListener('click', () => window.scrollTo({ top: 0, behavior: 'smooth' }));
}

/* ---- Lightbox ---- */
function wireLightbox() {
  const lb = document.querySelector('.lightbox');
  if (!lb) return;
  const img = lb.querySelector('img');
  document.querySelectorAll('[data-lightbox]').forEach(el => {
    el.style.cursor = 'zoom-in';
    el.addEventListener('click', () => {
      img.src = el.currentSrc || el.src || el.getAttribute('data-lightbox');
      img.alt = el.alt || '';
      lb.classList.add('show');
    });
  });
  lb.addEventListener('click', () => lb.classList.remove('show'));
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') lb.classList.remove('show');
  });
}

/* ---- YouTube lazy-load ---- */
function wireLazyYouTube() {
  document.querySelectorAll('[data-yt]').forEach(box => {
    const id = box.getAttribute('data-yt');
    const title = box.getAttribute('data-yt-title') || '';
    const thumb = document.createElement('img');
    thumb.alt = title;
    thumb.loading = 'lazy';
    // Progressive fallback chain: maxresdefault → sddefault → hqdefault.
    // NOTE: YouTube serves a valid 120x90 JPEG "no preview" placeholder as the
    // BODY of 404 responses, so browsers render it successfully and onerror
    // does not fire. We therefore detect the placeholder by its dimensions
    // (naturalWidth === 120) after load and step to the next fallback.
    const fallbacks = [
      `https://i.ytimg.com/vi/${id}/maxresdefault.jpg`,
      `https://i.ytimg.com/vi/${id}/sddefault.jpg`,
      `https://i.ytimg.com/vi/${id}/hqdefault.jpg`,
    ];
    let fbIdx = 0;
    const advance = () => {
      fbIdx += 1;
      if (fbIdx < fallbacks.length) {
        thumb.src = fallbacks[fbIdx];
      } else {
        thumb.onload = null;
        thumb.onerror = null;
      }
    };
    thumb.onerror = advance;
    thumb.onload = () => {
      // 120x90 is YouTube's "no preview available" placeholder.
      if (thumb.naturalWidth <= 120 && thumb.naturalHeight <= 90) {
        advance();
      }
    };
    thumb.src = fallbacks[0];
    const play = document.createElement('button');
    play.className = 'yt-play';
    play.type = 'button';
    play.setAttribute('aria-label', 'Play video');
    play.innerHTML = '<svg width="52" height="52" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg"><circle cx="12" cy="12" r="11" fill="rgba(0,0,0,.65)" stroke="rgba(255,255,255,.35)" stroke-width="1.2"/><path d="M10 8.5v7l6-3.5z" fill="#fff"/></svg>';
    // Always-present escape hatch: if the embed fails (file://, strict blockers,
    // YouTube Studio "allow embedding" turned off), users can still reach the video.
    const ytLink = document.createElement('a');
    ytLink.className = 'yt-ext';
    ytLink.href = `https://youtu.be/${id}`;
    ytLink.target = '_blank';
    ytLink.rel = 'noopener';
    ytLink.textContent = 'Watch on YouTube ↗';
    ytLink.addEventListener('click', e => e.stopPropagation());
    box.appendChild(thumb);
    box.appendChild(play);
    box.appendChild(ytLink);
    box.addEventListener('click', () => {
      const iframe = document.createElement('iframe');
      iframe.src = `https://www.youtube-nocookie.com/embed/${id}?autoplay=1&rel=0`;
      iframe.title = title;
      iframe.allow = 'accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share';
      iframe.referrerPolicy = 'strict-origin-when-cross-origin';
      iframe.allowFullscreen = true;
      iframe.frameBorder = 0;
      box.innerHTML = '';
      box.appendChild(iframe);
      // Re-add the external link overlay so users can still escape to YouTube if the embed errors.
      const ext = document.createElement('a');
      ext.className = 'yt-ext yt-ext-over';
      ext.href = `https://youtu.be/${id}`;
      ext.target = '_blank';
      ext.rel = 'noopener';
      ext.textContent = 'Open on YouTube ↗';
      box.appendChild(ext);
    }, { once: true });
  });
}

/* ---- GitHub stars (optional enhancement) ---- */
async function wireStars() {
  const el = document.querySelector('[data-gh-stars]');
  if (!el) return;
  try {
    const r = await fetch('https://api.github.com/repos/swaruplab/operon');
    if (!r.ok) return;
    const j = await r.json();
    if (typeof j.stargazers_count === 'number') {
      el.textContent = j.stargazers_count.toLocaleString();
    }
  } catch (e) { /* non-fatal */ }
}

/* ---- Protocol search (used on protocols page) ---- */
function wireProtocolFilter() {
  const input = document.querySelector('[data-protocol-search]');
  const pills = document.querySelectorAll('[data-protocol-cat]');
  const cards = document.querySelectorAll('[data-protocol-card]');
  if (!cards.length) return;

  let currentCat = 'all';
  const apply = () => {
    const q = (input?.value || '').trim().toLowerCase();
    cards.forEach(c => {
      const cat = c.getAttribute('data-category') || '';
      const name = (c.getAttribute('data-name') || '').toLowerCase();
      const desc = (c.textContent || '').toLowerCase();
      const matchCat = (currentCat === 'all') || cat === currentCat;
      const matchQuery = !q || name.includes(q) || desc.includes(q);
      c.style.display = (matchCat && matchQuery) ? '' : 'none';
    });
  };
  if (input) input.addEventListener('input', apply);
  pills.forEach(p => {
    p.addEventListener('click', () => {
      pills.forEach(x => x.classList.remove('active'));
      p.classList.add('active');
      currentCat = p.getAttribute('data-protocol-cat');
      apply();
    });
  });
}

/* ---- Hero slideshow: clickable chips jump to a slide ---- */
function wireHeroShowcase() {
  document.querySelectorAll('.hero-showcase').forEach(hs => {
    const chips = hs.querySelectorAll('.hs-chip');
    if (!chips.length) return;
    let resumeTimer = null;

    const jumpTo = (idx) => {
      hs.classList.add('manual');
      hs.dataset.active = String(idx);
      chips.forEach(c => c.classList.toggle('active', Number(c.dataset.slide) === idx));
      // Auto-resume the auto-rotation after 15s of no interaction.
      if (resumeTimer) clearTimeout(resumeTimer);
      resumeTimer = setTimeout(() => {
        hs.classList.remove('manual');
        delete hs.dataset.active;
        chips.forEach(c => c.classList.remove('active'));
      }, 15000);
    };

    chips.forEach(chip => {
      chip.addEventListener('click', () => {
        const idx = Number(chip.dataset.slide || 0);
        jumpTo(idx);
      });
    });

    // Keyboard: left/right arrows on a focused chip advance the slideshow.
    hs.addEventListener('keydown', (e) => {
      if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
      const active = document.activeElement;
      if (!active || !active.classList.contains('hs-chip')) return;
      const arr = Array.from(chips);
      const idx = arr.indexOf(active);
      const next = e.key === 'ArrowRight'
        ? (idx + 1) % arr.length
        : (idx - 1 + arr.length) % arr.length;
      arr[next].focus();
      jumpTo(next);
      e.preventDefault();
    });
  });
}

/* =========================================================
   Interactive modules (count-up, clipboard, GitHub stats,
   mode picker, chat playground, before/after, HPC estimator,
   protocol expand, shortcuts, tour modal, theme toggle,
   scrollyteller, FAQ accordion)
   ========================================================= */

/* ---- Animated count-up on scroll-in ---- */
function wireCountUp() {
  const els = document.querySelectorAll('[data-count-to]');
  if (!els.length) return;
  if (!('IntersectionObserver' in window) || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    els.forEach(el => {
      const to = Number(el.dataset.countTo);
      const suffix = el.dataset.countSuffix || '';
      el.textContent = to + suffix;
    });
    return;
  }
  const run = (el) => {
    const to = Number(el.dataset.countTo);
    const dur = Number(el.dataset.countDuration || 1200);
    const suffix = el.dataset.countSuffix || '';
    const start = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - start) / dur);
      // easeOutCubic
      const eased = 1 - Math.pow(1 - t, 3);
      const val = Math.round(to * eased);
      el.textContent = val.toLocaleString() + suffix;
      if (t < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  };
  const obs = new IntersectionObserver((entries) => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        run(e.target);
        obs.unobserve(e.target);
      }
    });
  }, { threshold: 0.3 });
  els.forEach(el => obs.observe(el));
}

/* ---- Auto-inject copy buttons on every <pre><code> ---- */
function wireCopyButtons() {
  if (!navigator.clipboard) return;
  document.querySelectorAll('pre').forEach(pre => {
    if (pre.querySelector('.copy-btn')) return;
    if (pre.hasAttribute('data-no-copy')) return;
    pre.style.position = pre.style.position || 'relative';
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'copy-btn';
    btn.setAttribute('aria-label', 'Copy to clipboard');
    btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg><span>Copy</span>';
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      const code = pre.querySelector('code')?.innerText || pre.innerText || '';
      try {
        await navigator.clipboard.writeText(code.replace(/\s+$/,''));
        btn.classList.add('copied');
        btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12l5 5L20 7"/></svg><span>Copied</span>';
        setTimeout(() => {
          btn.classList.remove('copied');
          btn.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h10"/></svg><span>Copy</span>';
        }, 1600);
      } catch (err) { /* clipboard denied */ }
    });
    pre.appendChild(btn);
  });
}

/* ---- Extended GitHub stats: stars, contributors, release ---- */
async function wireGithubStats() {
  const starTargets = document.querySelectorAll('[data-gh-stars]');
  const relTargets = document.querySelectorAll('[data-gh-release]');
  const contribTargets = document.querySelectorAll('[data-gh-contributors]');
  if (!starTargets.length && !relTargets.length && !contribTargets.length) return;

  try {
    const r = await fetch('https://api.github.com/repos/swaruplab/operon');
    if (!r.ok) return;
    const j = await r.json();
    if (typeof j.stargazers_count === 'number') {
      const n = j.stargazers_count;
      starTargets.forEach(el => { el.textContent = n.toLocaleString(); el.closest('.gh-chip')?.classList.add('has-data'); });
    }
  } catch (e) { /* ignore */ }

  if (relTargets.length) {
    try {
      const r = await fetch('https://api.github.com/repos/swaruplab/operon/releases/latest');
      if (r.ok) {
        const j = await r.json();
        const tag = (j.tag_name || '').replace(/^v/, '');
        if (tag) relTargets.forEach(el => { el.textContent = 'v' + tag; el.closest('.gh-chip')?.classList.add('has-data'); });
      }
    } catch (e) { /* ignore */ }
  }

  if (contribTargets.length) {
    try {
      const r = await fetch('https://api.github.com/repos/swaruplab/operon/contributors?per_page=1&anon=true', { headers: { Accept: 'application/vnd.github+json' } });
      if (r.ok) {
        // Total count is embedded in the Link header; fall back to array length
        const link = r.headers.get('Link') || '';
        let n = 0;
        const m = link.match(/&page=(\d+)>; rel="last"/);
        if (m) n = Number(m[1]);
        if (!n) { const arr = await r.json(); n = Array.isArray(arr) ? arr.length : 0; }
        if (n) contribTargets.forEach(el => { el.textContent = n.toString(); el.closest('.gh-chip')?.classList.add('has-data'); });
      }
    } catch (e) { /* ignore */ }
  }
}

/* ---- FAQ accordion enhancement (uses native <details>) ---- */
function wireFaqAccordion() {
  // Only one FAQ item open at a time per .faq-accordion group
  document.querySelectorAll('.faq-accordion').forEach(group => {
    const items = group.querySelectorAll('details');
    items.forEach(d => {
      d.addEventListener('toggle', () => {
        if (d.open) items.forEach(x => { if (x !== d) x.open = false; });
      });
    });
  });
}

/* ---- "Which AI mode?" picker ---- */
function wireModePicker() {
  const root = document.querySelector('[data-mode-picker]');
  if (!root) return;

  const questions = [
    {
      q: 'What stage are you at?',
      opts: [
        { t: 'I know exactly what I want — just do it', k: ['agent'] },
        { t: 'I want a plan I can review before anything runs', k: ['plan'] },
        { t: 'I just have a question', k: ['ask'] },
        { t: 'I finished an analysis and need a write-up', k: ['report'] }
      ]
    },
    {
      q: 'Should Claude touch your files?',
      opts: [
        { t: 'Yes — run commands, edit files, iterate', k: ['agent'] },
        { t: 'Only after I approve a plan', k: ['plan'] },
        { t: 'No — read-only is fine', k: ['ask', 'report'] }
      ]
    },
    {
      q: 'Do you need citations?',
      opts: [
        { t: 'Yes — ground answers in PubMed', k: ['ask', 'report'] },
        { t: 'No — working code / outputs matter more', k: ['agent', 'plan'] }
      ]
    }
  ];

  const modeInfo = {
    agent:  { name: 'Agent',  icon: '🤖', color: 'var(--accent)',  desc: 'Full tool use — reads, writes, runs commands, iterates until the plot is on disk.' },
    plan:   { name: 'Plan',   icon: '📋', color: '#eab308',        desc: 'Writes a step-by-step implementation_plan.md you can challenge before anything runs.' },
    ask:    { name: 'Ask',    icon: '💬', color: '#22c55e',        desc: 'Pure Q&A, optionally grounded in PubMed. No file writes.' },
    report: { name: 'Report', icon: '🧪', color: '#a855f7',        desc: 'Turns a finished session into a publication-ready PDF or Markdown write-up.' }
  };

  const state = { step: 0, votes: { agent:0, plan:0, ask:0, report:0 } };

  const render = () => {
    if (state.step < questions.length) {
      const q = questions[state.step];
      root.innerHTML = `
        <div class="mp-progress" role="progressbar" aria-valuemin="1" aria-valuemax="${questions.length}" aria-valuenow="${state.step+1}">
          ${questions.map((_, i) => `<span class="mp-step ${i <= state.step ? 'done' : ''}"></span>`).join('')}
        </div>
        <div class="mp-question">
          <div class="mp-step-label">Step ${state.step + 1} of ${questions.length}</div>
          <h3>${q.q}</h3>
          <div class="mp-opts">
            ${q.opts.map((o, i) => `<button type="button" class="mp-opt" data-opt="${i}">${o.t}</button>`).join('')}
          </div>
        </div>
      `;
      root.querySelectorAll('.mp-opt').forEach(btn => {
        btn.addEventListener('click', () => {
          const idx = Number(btn.dataset.opt);
          q.opts[idx].k.forEach(k => state.votes[k]++);
          state.step++;
          render();
        });
      });
    } else {
      const top = Object.entries(state.votes).sort((a,b) => b[1] - a[1])[0][0];
      const m = modeInfo[top];
      root.innerHTML = `
        <div class="mp-result">
          <div class="mp-result-badge" style="--mp-accent: ${m.color}"><span class="mp-ic">${m.icon}</span></div>
          <div class="mp-result-label">We'd use</div>
          <h3 class="mp-result-name">${m.name} mode</h3>
          <p class="mp-result-desc">${m.desc}</p>
          <div class="mp-result-actions">
            <button type="button" class="btn btn-primary btn-sm" data-mp-restart>Try again</button>
            <a href="guide.html#ai-modes" class="btn btn-ghost btn-sm">Read the full guide →</a>
          </div>
        </div>
      `;
      root.querySelector('[data-mp-restart]').addEventListener('click', () => {
        state.step = 0; state.votes = { agent:0, plan:0, ask:0, report:0 };
        render();
      });
    }
  };
  render();
}

/* ---- Scripted Claude chat playground ---- */
function wireChatPlayground() {
  const root = document.querySelector('[data-chat-playground]');
  if (!root) return;

  const presets = {
    analyze: {
      prompt: 'Cluster this Seurat object and find DEGs between clusters 3 and 7',
      thinking: 'Reading the Seurat object, checking normalization, planning a standard scRNA-seq workflow: PCA → UMAP → clustering → FindMarkers.',
      tools: [
        { name: 'Bash', input: 'Rscript -e "seurat <- readRDS(\'pbmc.rds\'); dim(seurat)"', output: '[1] 18563  2700' },
        { name: 'Edit', input: 'analysis.R', output: 'Added clustering + FindMarkers block' },
        { name: 'Bash', input: 'Rscript analysis.R', output: 'Wrote 412 DEGs to degs_3_vs_7.csv' }
      ],
      reply: "Done. I ran PCA on the top 2,000 variable genes, built a UMAP, and clustered at resolution 0.8. Between clusters 3 and 7, I found **412 differentially expressed genes** (FDR < 0.05, |log2FC| > 0.5).\n\nTop markers for cluster 3: IL7R, CCR7, LEF1 — looks like naive CD4+ T cells.\nTop markers for cluster 7: GNLY, GZMB, PRF1 — classic cytotoxic / NK signature.\n\nFull results in `degs_3_vs_7.csv`, UMAP in `plots/umap_clusters.pdf`."
    },
    pipeline: {
      prompt: 'Write a Snakemake pipeline for bulk RNA-seq: fastp → STAR → featureCounts → DESeq2',
      thinking: 'Drafting a DAG-based Snakemake with conda envs per rule. Need sample sheet format, genome index path, and output directory.',
      tools: [
        { name: 'Write', input: 'Snakefile', output: 'Created 6-rule pipeline' },
        { name: 'Write', input: 'config.yaml', output: 'Sample sheet + paths' },
        { name: 'Write', input: 'envs/deseq2.yaml', output: 'R + DESeq2 environment' }
      ],
      reply: "Done. I wrote a 6-rule Snakefile with per-rule conda envs so nothing clashes:\n\n• `fastp` trims + QC\n• `star_index` (one-off) → `star_align`\n• `featurecounts` → gene×sample matrix\n• `deseq2` → results table + MA/volcano plots\n\nSample sheet lives in `config.yaml`. Run with:\n\n```bash\nsnakemake --use-conda --cores 16\n```\n\nReady to test on your data — point me at the fastq directory and I'll set `config.yaml`."
    },
    pubmed: {
      prompt: 'Why did DESeq2 shrink my log2 fold changes so aggressively?',
      thinking: 'This is a question about DESeq2\'s shrinkage estimator. Likely about apeglm vs. normal. I should search PubMed for the canonical reference.',
      tools: [
        { name: 'PubMed', input: 'DESeq2 log fold change shrinkage apeglm', output: 'Found 3 papers' }
      ],
      reply: "DESeq2's default shrinkage uses **apeglm** (Zhu et al., 2018, [Bioinformatics](https://doi.org/10.1093/bioinformatics/bty895)), which pulls log2FC toward zero for genes with low counts or high dispersion — so LFCs you see from `results()` are the *shrunk* estimates, not the raw MLE.\n\nThree reasons the shrinkage feels aggressive:\n1. **Low counts** — genes with few reads carry little info; the prior dominates.\n2. **High variance** — noisy genes get pulled harder.\n3. **Small sample size** — with n=3 per group, the prior weighs more than the data.\n\nIf you want the raw MLE estimates, use `lfcShrink(..., type='normal')` or access `mcols(dds)$log2FoldChangeMLE`. But for ranking, visualizing, or downstream GSEA, the shrunk values are almost always what you want.",
      citations: [
        { title: 'Heavy-tailed prior distributions for sequence count data (apeglm)', year: 2018, journal: 'Bioinformatics', doi: '10.1093/bioinformatics/bty895' }
      ]
    },
    debug: {
      prompt: 'My STAR alignment is hitting segfault — see the log',
      thinking: 'Segfault in STAR usually means memory pressure or corrupted genome index. Let me check the log and the machine\'s memory.',
      tools: [
        { name: 'Read', input: 'logs/star_align.log', output: '...\nterminate called after throwing an instance of std::bad_alloc\nSegmentation fault' },
        { name: 'Bash', input: 'free -h', output: 'Mem: 64G total, 2.1G avail' },
        { name: 'Bash', input: 'ls -la genome/STAR_index/', output: 'SA: 26G, SAindex: 5G' }
      ],
      reply: "It's a memory issue. STAR's shared-memory index for a 26 GB SA file needs ~30 GB free, and you only have 2 GB available — the segfault is `std::bad_alloc` masquerading as a crash.\n\nFix options, in order of ease:\n1. **Close other jobs** on the machine to free memory.\n2. **Add `--limitBAMsortRAM 20000000000`** to cap sort memory.\n3. **Use `--genomeLoad NoSharedMemory`** — slower but each instance gets its own alloc.\n4. **Submit to HPC** with a 64 GB+ reservation — your local box is too tight.\n\nWant me to switch the Snakemake STAR rule to option 3 and retry?"
    }
  };

  const esc = (s) => s.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const renderMd = (s) => {
    // minimal markdown: **bold**, `code`, [link](url), line breaks, bullet lists
    let out = esc(s);
    out = out.replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>');
    out = out.replace(/`([^`]+)`/g, '<code>$1</code>');
    out = out.replace(/\[(.+?)\]\((https?:[^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');
    // code fences ```bash\ncode\n```
    out = out.replace(/```(\w+)?\n([\s\S]+?)\n```/g, (_, lang, code) => `</p><pre><code>${code}</code></pre><p>`);
    // paragraphs
    out = '<p>' + out.split(/\n\n+/).join('</p><p>') + '</p>';
    out = out.replace(/\n/g, '<br>');
    out = out.replace(/<p>\s*<\/p>/g, '');
    return out;
  };

  let currentTimers = [];
  const clearTimers = () => { currentTimers.forEach(t => clearTimeout(t)); currentTimers = []; };
  const after = (ms, fn) => { const t = setTimeout(fn, ms); currentTimers.push(t); return t; };

  const stream = (messagesEl, preset) => {
    // 1. User bubble
    const user = document.createElement('div');
    user.className = 'cp-msg cp-user';
    user.innerHTML = `<div class="cp-bubble">${esc(preset.prompt)}</div>`;
    messagesEl.appendChild(user);
    messagesEl.scrollTop = messagesEl.scrollHeight;

    // 2. Thinking block (collapsed after 1s delay)
    after(400, () => {
      const think = document.createElement('div');
      think.className = 'cp-msg cp-think';
      think.innerHTML = `
        <details class="cp-thinking" open>
          <summary><span class="cp-dot"></span> Thinking<span class="cp-dots-anim">...</span></summary>
          <div class="cp-think-body"></div>
        </details>`;
      messagesEl.appendChild(think);
      const body = think.querySelector('.cp-think-body');
      typewrite(body, preset.thinking, 18, () => {
        after(600, () => {
          think.querySelector('.cp-dots-anim').remove();
          think.querySelector('summary').insertAdjacentHTML('beforeend', ' <span class="cp-think-done">· done</span>');
          // Auto-collapse after thinking completes
          after(400, () => { think.querySelector('.cp-thinking').open = false; streamTools(); });
        });
      });
    });

    let toolIdx = 0;
    const streamTools = () => {
      if (toolIdx >= preset.tools.length) return streamReply();
      const tool = preset.tools[toolIdx];
      const el = document.createElement('div');
      el.className = 'cp-msg cp-tool';
      el.innerHTML = `
        <div class="cp-tool-card">
          <div class="cp-tool-head">
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 3l7 7-11 11H3v-7L14 3z"/></svg>
            <span class="cp-tool-name">${esc(tool.name)}</span>
            <span class="cp-tool-status">running<span class="cp-dots-anim">...</span></span>
          </div>
          <div class="cp-tool-body">
            <div class="cp-tool-in"><span class="cp-label">input</span><code></code></div>
            <div class="cp-tool-out" hidden><span class="cp-label">output</span><code></code></div>
          </div>
        </div>`;
      messagesEl.appendChild(el);
      messagesEl.scrollTop = messagesEl.scrollHeight;
      const inCode = el.querySelector('.cp-tool-in code');
      const outWrap = el.querySelector('.cp-tool-out');
      const outCode = el.querySelector('.cp-tool-out code');
      typewrite(inCode, tool.input, 8, () => {
        after(350, () => {
          outWrap.hidden = false;
          typewrite(outCode, tool.output, 4, () => {
            el.querySelector('.cp-tool-status').innerHTML = '<span class="cp-check">✓</span> done';
            toolIdx++;
            after(300, streamTools);
          });
        });
      });
    };

    const streamReply = () => {
      const el = document.createElement('div');
      el.className = 'cp-msg cp-assist';
      el.innerHTML = `<div class="cp-bubble"><div class="cp-reply"></div></div>`;
      messagesEl.appendChild(el);
      const body = el.querySelector('.cp-reply');
      typewriteRich(body, preset.reply, () => {
        if (preset.citations) {
          const cit = document.createElement('div');
          cit.className = 'cp-citations';
          cit.innerHTML = '<div class="cp-cit-label">Citations</div>' + preset.citations.map(c => `<a class="cp-cit" href="https://doi.org/${c.doi}" target="_blank" rel="noopener"><span class="cp-cit-title">${esc(c.title)}</span><span class="cp-cit-meta">${esc(c.journal)} · ${c.year}</span></a>`).join('');
          el.querySelector('.cp-bubble').appendChild(cit);
        }
        root.classList.remove('streaming');
      });
    };
  };

  const typewrite = (el, text, speed, done) => {
    let i = 0;
    const tick = () => {
      el.textContent = text.slice(0, i);
      if (i++ >= text.length) return done && done();
      after(speed, tick);
    };
    tick();
  };

  const typewriteRich = (el, markdown, done) => {
    // Render final HTML, then reveal character by character using clip/opacity trick.
    const html = renderMd(markdown);
    el.innerHTML = html;
    // Simple reveal: fade-in the whole block then scroll
    el.style.opacity = '0';
    after(60, () => {
      el.style.transition = 'opacity .4s ease';
      el.style.opacity = '1';
      // Scroll as content fills
      const scrollHost = el.closest('.cp-messages');
      if (scrollHost) scrollHost.scrollTop = scrollHost.scrollHeight;
      after(450, () => done && done());
    });
  };

  const startPreset = (key) => {
    clearTimers();
    const messagesEl = root.querySelector('.cp-messages');
    messagesEl.innerHTML = '';
    const preset = presets[key];
    if (!preset) return;
    root.classList.add('streaming');
    stream(messagesEl, preset);
  };

  root.innerHTML = `
    <div class="cp-panel">
      <div class="cp-head">
        <div class="cp-head-left">
          <span class="cp-brand-ic">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linejoin="round"><path d="M12 2l2.4 5.6L20 10l-5.6 2.4L12 18l-2.4-5.6L4 10l5.6-2.4L12 2z"/></svg>
          </span>
          <span class="cp-title">Claude</span>
          <span class="cp-pill">Agent</span>
        </div>
        <div class="cp-head-right">
          <span class="cp-model">claude-sonnet-4-6</span>
        </div>
      </div>
      <div class="cp-messages" role="log" aria-live="polite"></div>
      <div class="cp-presets">
        <div class="cp-presets-label">Try a sample prompt:</div>
        <div class="cp-presets-row">
          <button type="button" class="cp-preset" data-preset="analyze">🧬 Cluster a Seurat object</button>
          <button type="button" class="cp-preset" data-preset="pipeline">⚙️ Write a bulk RNA-seq pipeline</button>
          <button type="button" class="cp-preset" data-preset="pubmed">📚 Ask a literature question</button>
          <button type="button" class="cp-preset" data-preset="debug">🐛 Debug a STAR segfault</button>
        </div>
        <div class="cp-hint">This is a scripted demo. The real app streams live from Claude.</div>
      </div>
    </div>
  `;

  root.querySelectorAll('.cp-preset').forEach(b => {
    b.addEventListener('click', () => {
      root.querySelectorAll('.cp-preset').forEach(x => x.classList.remove('active'));
      b.classList.add('active');
      startPreset(b.dataset.preset);
    });
  });

  // Auto-start first preset when scrolled into view
  if ('IntersectionObserver' in window) {
    const obs = new IntersectionObserver((entries) => {
      entries.forEach(e => {
        if (e.isIntersecting) {
          const first = root.querySelector('.cp-preset');
          if (first && !first.classList.contains('active')) first.click();
          obs.unobserve(e.target);
        }
      });
    }, { threshold: 0.3 });
    obs.observe(root);
  }
}

/* ---- Before/After drag-to-reveal slider ---- */
function wireBeforeAfter() {
  document.querySelectorAll('[data-ba-slider]').forEach(root => {
    const divider = root.querySelector('.ba-divider');
    const after = root.querySelector('.ba-after');
    if (!divider || !after) return;

    let pct = 50;
    const apply = () => {
      after.style.clipPath = `inset(0 0 0 ${pct}%)`;
      divider.style.left = pct + '%';
    };
    apply();

    const setFromEvent = (e) => {
      const rect = root.getBoundingClientRect();
      const x = (e.touches ? e.touches[0].clientX : e.clientX) - rect.left;
      pct = Math.max(0, Math.min(100, (x / rect.width) * 100));
      apply();
    };

    let dragging = false;
    const onDown = (e) => { dragging = true; setFromEvent(e); e.preventDefault(); };
    const onMove = (e) => { if (dragging) setFromEvent(e); };
    const onUp = () => { dragging = false; };
    divider.addEventListener('mousedown', onDown);
    root.addEventListener('mousedown', onDown);
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    divider.addEventListener('touchstart', onDown, { passive: false });
    window.addEventListener('touchmove', onMove, { passive: true });
    window.addEventListener('touchend', onUp);

    // Keyboard support
    divider.setAttribute('tabindex', '0');
    divider.setAttribute('role', 'slider');
    divider.setAttribute('aria-label', 'Drag to compare before and after');
    divider.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowLeft')  { pct = Math.max(0, pct - 4);   apply(); e.preventDefault(); }
      if (e.key === 'ArrowRight') { pct = Math.min(100, pct + 4); apply(); e.preventDefault(); }
    });
  });
}

/* ---- HPC cost/time estimator ---- */
function wireHpcEstimator() {
  const root = document.querySelector('[data-hpc-estimator]');
  if (!root) return;

  const controls = {
    samples: root.querySelector('[data-hpc-samples]'),
    size:    root.querySelector('[data-hpc-size]'),
    type:    root.querySelector('[data-hpc-type]')
  };
  if (!controls.samples || !controls.size || !controls.type) return;

  const out = {
    hours:     root.querySelector('[data-hpc-hours]'),
    partition: root.querySelector('[data-hpc-partition]'),
    cores:     root.querySelector('[data-hpc-cores]'),
    memory:    root.querySelector('[data-hpc-memory]'),
    speedup:   root.querySelector('[data-hpc-speedup]'),
    echoSamples: root.querySelector('[data-hpc-echo-samples]'),
    echoSize:    root.querySelector('[data-hpc-echo-size]'),
    echoType:    root.querySelector('[data-hpc-echo-type]')
  };

  const profiles = {
    scrna:  { name: 'scRNA-seq (Seurat + CellRanger)', baseHours: 2.5,  coresPer: 8,  memPerGB: 1.2,  partition: 'standard', laptopMult: 6 },
    bulk:   { name: 'Bulk RNA-seq (STAR + DESeq2)',    baseHours: 0.8,  coresPer: 4,  memPerGB: 0.3,  partition: 'standard', laptopMult: 4 },
    atac:   { name: 'ATAC-seq (Bowtie2 + MACS2)',      baseHours: 1.2,  coresPer: 6,  memPerGB: 0.4,  partition: 'standard', laptopMult: 5 },
    wgs:    { name: 'Whole-genome (BWA + GATK)',       baseHours: 8.0,  coresPer: 16, memPerGB: 2.0,  partition: 'highmem',  laptopMult: 10 },
    spatial:{ name: 'Spatial (Space Ranger)',          baseHours: 4.0,  coresPer: 12, memPerGB: 1.5,  partition: 'highmem',  laptopMult: 8 }
  };

  const humanTime = (h) => {
    if (h < 1) return Math.round(h * 60) + ' min';
    if (h < 24) return h.toFixed(1).replace(/\.0$/,'') + ' h';
    const d = h / 24;
    return d.toFixed(1).replace(/\.0$/,'') + ' d';
  };

  const recompute = () => {
    const n = Number(controls.samples.value);
    const gb = Number(controls.size.value);
    const t = controls.type.value;
    const p = profiles[t] || profiles.bulk;

    // Simple model: hours scale with samples * (size / 10GB)^0.7
    const sizeFactor = Math.pow(gb / 10, 0.7);
    const hpcHours = p.baseHours * n * sizeFactor;
    const laptopHours = hpcHours * p.laptopMult;

    out.hours.textContent = humanTime(hpcHours);
    out.partition.textContent = p.partition;
    out.cores.textContent = p.coresPer + ' cores';
    out.memory.textContent = Math.max(16, Math.round(p.memPerGB * gb * n / 4)) + ' GB';
    out.speedup.textContent = humanTime(laptopHours) + ' → ' + humanTime(hpcHours);
    if (out.echoSamples) out.echoSamples.textContent = n;
    if (out.echoSize) out.echoSize.textContent = gb;
    if (out.echoType) out.echoType.textContent = p.name;
  };

  ['input','change'].forEach(ev => {
    controls.samples.addEventListener(ev, recompute);
    controls.size.addEventListener(ev, recompute);
    controls.type.addEventListener(ev, recompute);
  });
  recompute();
}

/* ---- Expandable protocol cards ----
   Adds a "View details" affordance + generated body to every
   [data-protocol-card] that doesn't already have [data-expand-body].
---- */
function wireProtocolExpand() {
  document.querySelectorAll('[data-protocol-card]').forEach(card => {
    let body = card.querySelector('[data-expand-body]');

    // Auto-inject body if not present
    if (!body) {
      const name = card.querySelector('h3')?.textContent?.trim() || 'Protocol';
      const meta = card.querySelector('.proto-meta')?.innerHTML || '';
      body = document.createElement('div');
      body.className = 'protocol-body';
      body.setAttribute('data-expand-body', '');
      body.innerHTML = `
        <div style="font-size:.82rem; color: var(--text-3); margin-bottom: 12px;">
          <strong style="color: var(--text); font-weight: 600;">Typical pipeline:</strong>
          QC &rarr; preprocessing &rarr; main analysis &rarr; QC gates &rarr; figures &rarr; report
        </div>
        <div style="font-size:.82rem; color: var(--text-3); margin-bottom: 14px;">
          <strong style="color: var(--text); font-weight: 600;">Stack:</strong> ${meta}
        </div>
        <div class="row" style="gap: 8px; flex-wrap: wrap;">
          <a href="download.html" class="btn btn-primary btn-sm">Open in Operon &rarr;</a>
          <a href="https://github.com/swaruplab/operon/tree/main/protocols" target="_blank" rel="noopener" class="btn btn-ghost btn-sm">Source on GitHub</a>
        </div>
      `;
      card.appendChild(body);
    }

    // Chevron indicator
    if (!card.querySelector('.protocol-expand-chevron')) {
      const chev = document.createElement('span');
      chev.className = 'protocol-expand-chevron';
      chev.setAttribute('aria-hidden', 'true');
      chev.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 9l6 6 6-6"/></svg>';
      chev.style.cssText = 'display:inline-block; margin-left:auto; color:var(--text-3); align-self:flex-start;';
      const h3 = card.querySelector('h3');
      if (h3) {
        h3.style.display = 'flex';
        h3.style.alignItems = 'center';
        h3.style.gap = '8px';
        h3.appendChild(chev);
      }
    }

    card.setAttribute('data-expand-trigger', '');
    card.setAttribute('role', 'button');
    card.setAttribute('tabindex', '0');
    card.setAttribute('aria-expanded', 'false');

    const toggle = () => {
      const isOpen = card.classList.toggle('expanded');
      card.setAttribute('aria-expanded', isOpen ? 'true' : 'false');
    };
    card.addEventListener('click', (e) => {
      // Don't hijack clicks on inner links/buttons
      if (e.target.closest('a, button')) return;
      toggle();
    });
    card.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); }
    });
  });
}

/* ---- Interactive keyboard shortcut cheatsheet ---- */
function wireKbdCheatsheet() {
  const root = document.querySelector('[data-kbd-cheatsheet]');
  if (!root) return;
  const cards = root.querySelectorAll('[data-kbd-keys]');
  const search = root.querySelector('[data-kbd-search]');

  const normalize = (k) => k.toLowerCase().replace('arrowleft','←').replace('arrowright','→').replace('arrowup','↑').replace('arrowdown','↓');

  const applyFilter = () => {
    const q = (search?.value || '').trim().toLowerCase();
    cards.forEach(card => {
      const keys = card.dataset.kbdKeys.toLowerCase();
      const label = card.textContent.toLowerCase();
      card.style.display = (!q || keys.includes(q) || label.includes(q)) ? '' : 'none';
    });
  };
  if (search) search.addEventListener('input', applyFilter);

  // Highlight matching shortcut when key is pressed
  document.addEventListener('keydown', (e) => {
    // Don't hijack when typing in inputs
    if (/input|textarea/i.test(e.target.tagName)) return;
    if (!root.matches(':is(:hover, :focus-within)') && !isInView(root)) return;

    const parts = [];
    if (e.metaKey || e.ctrlKey) parts.push('cmd');
    if (e.shiftKey) parts.push('shift');
    if (e.altKey) parts.push('alt');
    if (e.key.length === 1) parts.push(e.key.toLowerCase());
    else parts.push(normalize(e.key));

    const combo = parts.join('+');
    cards.forEach(card => {
      const keys = card.dataset.kbdKeys.toLowerCase().replace(/\s+/g,'');
      card.classList.toggle('pressed', keys === combo);
    });
    setTimeout(() => cards.forEach(c => c.classList.remove('pressed')), 400);
  });

  const isInView = (el) => {
    const r = el.getBoundingClientRect();
    return r.top < window.innerHeight && r.bottom > 0;
  };
}

/* ---- 60-second guided tour modal ---- */
function wireTourModal() {
  const trigger = document.querySelector('[data-tour-trigger]');
  const modal = document.querySelector('[data-tour-modal]');
  if (!trigger || !modal) return;

  const steps = JSON.parse(modal.dataset.tourSteps || '[]');
  let idx = 0;

  const stage = modal.querySelector('.tour-stage');
  const title = modal.querySelector('.tour-title');
  const desc = modal.querySelector('.tour-desc');
  const progress = modal.querySelector('.tour-progress');
  const stepLabel = modal.querySelector('.tour-step-label');
  const prevBtn = modal.querySelector('[data-tour-prev]');
  const nextBtn = modal.querySelector('[data-tour-next]');
  const closeBtn = modal.querySelector('[data-tour-close]');
  const dotsEl = modal.querySelector('.tour-dots');

  const render = () => {
    const step = steps[idx];
    if (!step) return;
    if (stage) stage.style.backgroundImage = `url('${step.img}')`;
    title.textContent = step.title;
    desc.textContent = step.desc;
    stepLabel.textContent = `${idx + 1} / ${steps.length}`;
    prevBtn.disabled = (idx === 0);
    nextBtn.textContent = (idx === steps.length - 1) ? 'Finish' : 'Next →';
    if (progress) progress.style.width = ((idx + 1) / steps.length * 100) + '%';
    dotsEl.innerHTML = steps.map((_, i) => `<span class="tour-dot ${i === idx ? 'active' : ''}"></span>`).join('');
    // Position hotspot highlight if step defines coords
    const hs = modal.querySelector('.tour-hotspot');
    if (hs) {
      if (step.hotspot) {
        hs.style.display = '';
        hs.style.left = step.hotspot.x + '%';
        hs.style.top = step.hotspot.y + '%';
        hs.style.width = (step.hotspot.w || 20) + '%';
        hs.style.height = (step.hotspot.h || 20) + '%';
      } else {
        hs.style.display = 'none';
      }
    }
  };

  const open = () => { modal.classList.add('show'); idx = 0; render(); document.body.classList.add('no-scroll'); };
  const close = () => { modal.classList.remove('show'); document.body.classList.remove('no-scroll'); };

  trigger.addEventListener('click', (e) => { e.preventDefault(); open(); });
  closeBtn.addEventListener('click', close);
  prevBtn.addEventListener('click', () => { if (idx > 0) { idx--; render(); } });
  nextBtn.addEventListener('click', () => { if (idx < steps.length - 1) { idx++; render(); } else close(); });
  modal.addEventListener('click', (e) => { if (e.target === modal) close(); });
  document.addEventListener('keydown', (e) => {
    if (!modal.classList.contains('show')) return;
    if (e.key === 'Escape') close();
    if (e.key === 'ArrowRight' && idx < steps.length - 1) { idx++; render(); }
    if (e.key === 'ArrowLeft' && idx > 0) { idx--; render(); }
  });
}

/* ---- Light / dark theme toggle ---- */
function wireThemeToggle() {
  const root = document.documentElement;
  const stored = localStorage.getItem('operon-theme');
  if (stored === 'light' || stored === 'dark') root.setAttribute('data-theme', stored);

  document.querySelectorAll('[data-theme-toggle]').forEach(btn => {
    const apply = () => {
      const t = root.getAttribute('data-theme') || 'dark';
      btn.setAttribute('aria-label', t === 'light' ? 'Switch to dark mode' : 'Switch to light mode');
      btn.classList.toggle('is-light', t === 'light');
    };
    apply();
    btn.addEventListener('click', () => {
      const current = root.getAttribute('data-theme') || 'dark';
      const next = current === 'light' ? 'dark' : 'light';
      root.setAttribute('data-theme', next);
      localStorage.setItem('operon-theme', next);
      apply();
    });
  });
}

/* ---- Scrollytelling: sticky image cross-fades as sections scroll ---- */
function wireScrollyteller() {
  const root = document.querySelector('[data-scrollyteller]');
  if (!root) return;
  const steps = root.querySelectorAll('[data-scrolly-step]');
  const stage = root.querySelector('[data-scrolly-stage]');
  if (!steps.length || !stage) return;

  const setActive = (i) => {
    steps.forEach(s => s.classList.toggle('active', Number(s.dataset.scrollyStep) === i));
    const layers = stage.querySelectorAll('[data-scrolly-layer]');
    layers.forEach(l => l.classList.toggle('active', Number(l.dataset.scrollyLayer) === i));
  };

  if (!('IntersectionObserver' in window)) {
    setActive(0);
    return;
  }
  const obs = new IntersectionObserver((entries) => {
    entries.forEach(e => {
      if (e.isIntersecting) {
        const i = Number(e.target.dataset.scrollyStep);
        setActive(i);
      }
    });
  }, { rootMargin: '-40% 0px -40% 0px', threshold: 0 });
  steps.forEach(s => obs.observe(s));
  setActive(0);
}

/* ---- Init ---- */
document.addEventListener('DOMContentLoaded', () => {
  loadDownloads();
  wireDownloadDropdown();
  wireNav();
  wireReveal();
  wireBackTop();
  wireLightbox();
  wireLazyYouTube();
  wireGithubStats();
  wireProtocolFilter();
  wireHeroShowcase();
  wireCountUp();
  wireCopyButtons();
  wireFaqAccordion();
  wireModePicker();
  wireChatPlayground();
  wireBeforeAfter();
  wireHpcEstimator();
  wireProtocolExpand();
  wireKbdCheatsheet();
  wireTourModal();
  wireThemeToggle();
  wireScrollyteller();
});
