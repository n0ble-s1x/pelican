/* Pelican — frontend behaviour. No framework, no bundler, no dependencies.

   Every fact on screen now comes from pelican-core, over one Tauri Channel.
   This layer still owns presentation only: it renders what the engine says
   and never decides anything the device is the authority on.

   Opened in a plain browser instead of the app, it falls back to the URL
   hash so the three states stay reviewable without hardware:
     #connected  #disconnected  #transferring                              */

(() => {
  'use strict';

  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];

  const app = $('.app');
  const transfer = $('.transfer');
  const selection = $('[data-selection]');
  const water = $('.water');
  const pending = $('.water__pending');
  const status = $('.device__status');
  const deviceName = $('.device__name');
  const sendWhy = $('[data-send-why]');
  const prewarn = $('.prewarn:not([data-encoder]):not([data-error])');
  const encWarn = $('[data-encoder]');
  const errBox = $('[data-error]');
  const meter = $('[data-meter]');
  const rowsEl = $('[data-rows]');
  const wall = $('.wall');
  const wallList = $('[data-wall-list]');
  const wallCount = $('[data-wall-count]');
  const audio = $('#audio');
  const seek = $('.seek');
  const playpause = $('.playpause');
  const sendBtn = $('[data-send]');

  /* ── state ──────────────────────────────────────────────────────────── */

  /* The single source of truth, fed entirely by the event channel. Nothing
     here is a guess: `free`/`total` are what the watch reports, `bytes` per
     track is what the planner says the file will weigh after conversion. */
  const state = {
    attached: null,        /* { label, serial, product } or null */
    connected: false,
    /* The watch's own model name, and it only exists once an MTP session is
       open: this device declares no USB product string (iProduct = 0), so
       the name lives in GetDeviceInfo and nowhere else. */
    model: null,
    /* Set when an open finished and failed. Without it the UI sat on
       "Opening your watch…" forever after a failure that had already
       happened. */
    openFailed: null,
    free: 0, total: 0,
    entries: [], uploads: [],
    tracks: [], root: null,
    encoder: null, encoderVerified: false,
    contention: null,
    syncing: false,
  };

  /* ── IPC ────────────────────────────────────────────────────────────── */

  const NATIVE = !!(window.__TAURI__ && window.__TAURI__.core);
  const core = NATIVE ? window.__TAURI__.core : null;

  const invoke = (cmd, args) =>
    NATIVE ? core.invoke(cmd, args)
           : Promise.reject(new Error('not running inside Pelican'));

  /* ── formatting ─────────────────────────────────────────────────────── */

  /* One ladder for every size in the app — the capacity headline, each wall
     row, the selection readout and the live byte counter all read off this,
     so they always reconcile against each other. Base-1000, because that is
     what the watch reports its own capacity in. `fmtParts` exists so the
     capacity block can put the unit in its own element without parsing the
     formatted string back apart; a KB tier is required because a real track
     on this watch weighs 49,635 bytes and "0 MB" is a false weight. */
  const fmtParts = (bytes) => {
    if (bytes >= 999.95e6) return { n: (bytes / 1e9).toFixed(2), unit: 'GB' };
    if (bytes >= 999.5e3) return { n: (bytes / 1e6).toFixed(1), unit: 'MB' };
    return { n: String(Math.round(bytes / 1e3)), unit: 'KB' };
  };

  const fmt = (bytes) => { const p = fmtParts(bytes); return `${p.n} ${p.unit}`; };

  const mmss = (secs) => {
    if (!Number.isFinite(secs) || secs < 0) secs = 0;
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return m + ':' + String(s).padStart(2, '0');
  };

  const plural = (n, word) => `${n} ${word}${n === 1 ? '' : 's'}`;

  /* Sizes come from the plan, not from the source file: a FLAC that will be
     converted lands far smaller than it starts. */
  const sizeOf = (row) => Number(row.dataset.bytes || 0);

  /* Drives transform, not height — see app.css. The element is full height and
     slides down to the level, so nothing lays out on an animation frame. */
  const setLevel = (el, pct) => {
    if (el) el.style.transform = `translateY(${(100 - pct).toFixed(2)}%)`;
  };

  /* ── selection ──────────────────────────────────────────────────────── */

  let picks = [];

  const checked = () => picks.filter((p) => p.checked).map((p) => p.closest('tr'));

  /* Tracks that will transfer but stay invisible on the watch, because they
     carry no title or artist. Surfaced at selection time, not mid-send. */
  const untagged = () => checked().filter((r) => r.dataset.untagged === 'true').length;

  function updateSelection() {
    const rows = checked();
    const bytes = rows.reduce((n, r) => n + sizeOf(r), 0);
    const known = state.total > 0;
    const fits = !known || bytes <= state.free;
    /* Anything that has to be converted has a *modelled* size — duration at
       the target bitrate — not a measured one, and scan.rs says so in its own
       doc comment. Two significant figures on a projection reads as a
       measurement, so the projection says it is one. */
    const est = rows.some((r) => r.dataset.pipeline && r.dataset.pipeline !== 'passthrough');
    const about = est ? 'about ' : '';

    /* The band sits on top of the current level and is measured against
       total capacity, so it answers "will this fit" directly. */
    if (pending && known) {
      const usedPct = (state.total - state.free) / state.total * 100;
      pending.style.bottom = usedPct.toFixed(2) + '%';
      const pct = Math.min(100, bytes / state.total * 100);
      pending.style.height = pct.toFixed(2) + '%';
      pending.style.opacity = pct > 0 ? '1' : '0';
    } else if (pending) {
      pending.style.height = '0%';
      pending.style.opacity = '0';
    }

    if (selection) {
      if (!rows.length) {
        selection.textContent = 'Nothing selected';
        selection.removeAttribute('data-fits');
      } else if (!known) {
        selection.textContent = `${rows.length} selected · ${about}${fmt(bytes)}`;
        selection.removeAttribute('data-fits');
      } else if (fits) {
        selection.textContent =
          `${rows.length} selected · ${about}${fmt(bytes)} · ${about}${fmt(state.free - bytes)} would remain`;
        selection.dataset.fits = 'yes';
      } else {
        selection.textContent =
          `${rows.length} selected · ${about}${fmt(bytes)} · ${about}${fmt(bytes - state.free)} too much`;
        selection.dataset.fits = 'no';
      }
    }

    if (sendBtn) {
      sendBtn.textContent = rows.length ? `Send ${plural(rows.length, 'track')}` : 'Send';
      sendBtn.disabled =
        !rows.length || !fits || app.dataset.device === 'none' || state.syncing;
    }

    /* A disabled control that does not say why is a dead end. The reason is
       rendered in the DOM and referenced by aria-describedby, so it reaches
       pointer and screen reader alike. */
    if (sendWhy) {
      let why = '';
      let blocked = 'reason';
      /* With no folder open there is nothing tickable, the hero already says
         so, and a red line under it would be the app alarming about its own
         first-run state. */
      if (!state.tracks.length) why = '';
      else if (state.syncing) why = 'A transfer is already running.';
      else if (app.dataset.device === 'none') why = 'Connect your watch to send music.';
      else if (!rows.length) why = 'Tick a track to send it.';
      else if (!fits) {
        why = `That is ${about}${fmt(bytes - state.free)} more than your watch has room for.`;
        blocked = 'capacity';
      }
      sendWhy.textContent = why;
      /* Signal Red is spent on the over-capacity case only — the one reason
         on DESIGN.md's list for that ink. The rest are quiet statements of
         fact, not failures. */
      sendWhy.dataset.blocked = blocked;
      sendWhy.hidden = !why;
    }

    if (prewarn) {
      const n = untagged();
      prewarn.hidden = n === 0;
      if (n) {
        $('.prewarn__tag', prewarn).textContent =
          `${plural(n, 'track')} need${n === 1 ? 's' : ''} a title`;
        const names = checked()
          .filter((r) => r.dataset.untagged === 'true')
          .slice(0, 3)
          .map((r) => r.dataset.name);
        const body = $('[data-prewarn-body]');
        /* The claim is precise and it is true: skip_tag_check is on, so the
           file really does transfer — it just never appears in the watch's
           music app. Weakening either half would misinform. */
        body.textContent = '';
        const strong = document.createElement('strong');
        strong.textContent = names.join(', ') + (n > names.length ? ` and ${n - names.length} more` : '');
        body.append(strong, document.createTextNode(
          n === 1
            ? ' has no title or artist. It will copy across, but your watch will never show it in the music app.'
            : ' have no title or artist. They will copy across, but your watch will never show them in the music app.'));
      }
    }
  }

  /* ── library ────────────────────────────────────────────────────────── */

  /* Keyed on the container, because the container is all Pelican reads. An
     `.ogg` may hold Vorbis or Opus and nothing in the pipeline opens it to
     find out; `.oga` is the same container under another name and was
     playing silently. */
  const OGG_SILENT = new Set(['ogg', 'oga']);

  const extOf = (path) => {
    const i = path.lastIndexOf('.');
    return i < 0 ? '' : path.slice(i + 1).toLowerCase();
  };

  function renderTracks(tracks) {
    const tpl = $('#tpl-track');
    rowsEl.textContent = '';
    for (const t of tracks) {
      const frag = tpl.content.cloneNode(true);
      const tr = frag.querySelector('tr');
      const name = t.path.split('/').pop();

      tr.dataset.path = t.path;
      tr.dataset.name = name;
      tr.dataset.bytes = String(t.bytes);
      tr.dataset.fmt = t.fmt;
      tr.dataset.pipeline = t.pipeline;
      if (!t.playableInLibrary) tr.dataset.untagged = 'true';

      const label = t.artist ? `${t.title} — ${t.artist}` : t.title;
      $('.t', tr).textContent = t.title;
      /* The format column is a fixed 96px in the approved comp, so anything
         longer than a format name belongs on the subtitle line instead — it
         wraps to three lines and drags the whole row's height with it. */
      /* A duration of zero is what scan.rs writes when lofty could not open
         the file at all. Unknown and zero are different facts, so the
         unknown one is left unsaid rather than printed as `0:00`. */
      $('.tracks__title .s', tr).textContent =
        (t.durationSecs ? `${mmss(t.durationSecs)} · ` : '')
        + (t.artist ? t.artist : 'no artist')
        + (t.sendable && t.pipeline !== 'passthrough' ? ' · converts to 192 kbps' : '')
        + (t.sendable ? '' : ' · cannot be converted for your watch');
      $('.tracks__fmt', tr).textContent = t.fmt;

      const play = $('.tracks__play .iconbtn', tr);
      play.setAttribute('aria-label', 'Play ' + label);
      /* Ogg Vorbis is excluded rather than untested: the media element
         reports readyState 4, a correct duration and a moving clock, and
         emits digital silence. That failure is invisible from its own API,
         so it has to be refused here. */
      if (OGG_SILENT.has(extOf(t.path))) {
        play.disabled = true;
        play.title = 'Pelican cannot preview Ogg files on this Mac.';
      }

      const pick = $('input', tr);
      pick.setAttribute('aria-label', 'Send ' + label);
      /* The engine already wrote the honest sentence for both cases — the
         refusal and the conversion profile. Showing it only for refusals
         threw the conversion disclosure away. */
      if (t.note) pick.title = t.note;
      if (!t.sendable) {
        pick.disabled = true;
        pick.setAttribute('aria-label', `Cannot send ${label}`);
      }

      rowsEl.appendChild(frag);
    }

    picks = $$('.tracks__pick input');
    picks.forEach((p) => p.addEventListener('change', updateSelection));
    wirePlay();
    updateSelection();
  }

  function renderHero() {
    const n = state.tracks.length;
    const title = $('[data-hero-title]');
    const by = $('[data-hero-by]');
    const facts = $('[data-hero-facts]');

    if (!state.root) {
      title.textContent = 'No folder open';
      by.textContent = 'Choose a folder of music to get started.';
      facts.textContent = '';
      return;
    }
    title.textContent = state.root.split('/').filter(Boolean).pop() || state.root;

    const artists = [...new Set(state.tracks.map((t) => t.artist).filter(Boolean))];
    by.textContent = artists.length === 0 ? 'No artist tags in this folder'
      : artists.length <= 3 ? artists.join(', ')
      : `${artists.slice(0, 2).join(', ')} and ${artists.length - 2} more`;

    const secs = state.tracks.reduce((s, t) => s + (t.durationSecs || 0), 0);
    const hrs = Math.floor(secs / 3600);
    const mins = Math.round((secs % 3600) / 60);
    const fmts = [...new Set(state.tracks.map((t) => t.fmt.split(' ')[0]))];
    facts.textContent = [
      plural(n, 'track'),
      secs ? (hrs ? `${hrs} hr ${mins} min` : `${mins} min`) : null,
      fmts.length > 3 ? `${fmts.slice(0, 3).join(' · ')} +${fmts.length - 3} more`
        : fmts.join(' · '),
    ].filter(Boolean).join(' · ');
  }

  function renderEncoderNote() {
    if (!encWarn) return;
    const needs = state.tracks.some((t) => t.pipeline === 'ffmpeg' || t.pipeline === 'afconvert');
    const refused = state.tracks.filter((t) => !t.sendable).length;
    if (!needs && !refused) { encWarn.hidden = true; return; }

    encWarn.hidden = false;
    const tag = $('.prewarn__tag', encWarn);
    const body = $('[data-encoder-body]');

    if (!state.encoder) {
      /* Not "not installed": what was observed is that Pelican could not spawn
         an encoder off its own PATH. scan.rs:179 keeps the same discipline. */
      tag.textContent = 'No converter Pelican can run';
      body.textContent =
        `${plural(refused, 'file')} in this folder ${refused === 1 ? 'is' : 'are'} in a format ` +
        'your watch cannot play, and Pelican found no converter it can run. Install ffmpeg.';
      return;
    }
    if (state.encoderVerified) {
      tag.textContent = `Converting with ${state.encoder}`;
      body.textContent =
        'Files your watch cannot play will be converted to 192 kbps MP3 on this Mac. ' +
        'This is the profile confirmed on a Forerunner 165.' +
        (refused ? ` ${plural(refused, 'file')} still cannot be converted at all.` : '');
      return;
    }
    /* Naming the untested path is the point. afconvert produces AAC that
       Garmin lists as playable, but no watch has confirmed it — presenting
       it as equivalent to the verified profile would be a claim we have not
       earned. What the engine actually observed is narrower than "ffmpeg is
       not installed": it tried to spawn `ffmpeg` off this process's PATH and
       failed, and a Finder-launched .app does not inherit the PATH a
       Homebrew ffmpeg lives on. Say the observation, not the inference. */
    tag.textContent = `Converting with ${state.encoder} — unconfirmed`;
    body.textContent =
      'Pelican could not find ffmpeg on its PATH, so conversions use macOS’s own ' +
      'afconvert and land as AAC in M4A. Garmin lists that format as playable, but ' +
      'Pelican has not confirmed it on a real watch. An app launched from Finder does ' +
      'not see a Homebrew ffmpeg.' +
      (refused ? ` ${plural(refused, 'file')} cannot be converted at all.` : '');
  }

  /* ── the wall ───────────────────────────────────────────────────────── */

  const stemOf = (name) => {
    const i = name.lastIndexOf('.');
    return (i <= 0 ? name : name.slice(0, i)).toLowerCase();
  };

  /* Two sources, and they are not equally good. What the device reports is
     observed: the handle came back from `list_dir("Music")` and its metadata
     read. What the journal holds is inferred — a local record that Pelican
     wrote when an upload completed, which is not evidence the file is still
     there. On the attached FR165 `/Music` is durable (all 20 of the owner's
     tracks sit in it across sessions), so a journal row the listing does not
     confirm most likely means the file has gone, and it says so rather than
     being counted as aboard.

     The dedupe key is the *sanitised remote stem* on both sides: the shell
     journals the name it actually wrote to the device, so `u.name` and
     `stemOf(e.name)` are the same string by construction.

     Broken stubs come first — they are the only rows with anything to do. */
  function wallItems() {
    const seen = new Set();
    const broken = [];
    const onDevice = [];
    for (const e of state.entries) {
      if (e.isFolder) continue;
      seen.add(stemOf(e.name));
      (e.isBroken ? broken : onDevice).push({
        broken: e.isBroken,
        /* Only a device entry carries an extension; a journal name is
           already a stem, and cutting it at its last dot would eat half of
           `Vol. 1 - Opening`. */
        name: e.isBroken ? e.name : stripExt(e.name),
        raw: e.name,
        path: e.path,
        bytes: e.size,
        onDevice: true,
        note: e.isBroken ? null : 'In /Music on the watch',
      });
    }
    const journal = [];
    for (const u of state.uploads) {
      if (seen.has(u.name.toLowerCase())) continue;
      journal.push({
        broken: false, name: u.name, raw: u.name, bytes: u.bytes, at: u.at,
        onDevice: false,
        note: `${sentWhen(u.at)} · the watch is not listing it now`,
      });
    }
    journal.sort((a, b) => (b.at || 0) - (a.at || 0));
    return [...broken, ...onDevice, ...journal];
  }

  function sentWhen(secs) {
    if (!secs) return 'Sent';
    const d = new Date(secs * 1000);
    const days = Math.floor((Date.now() - d.getTime()) / 86400000);
    if (days <= 0) return 'Sent today';
    if (days === 1) return 'Sent yesterday';
    if (days < 30) return `Sent ${days} days ago`;
    return 'Sent ' + d.toLocaleDateString();
  }

  function renderWall() {
    const items = wallItems();
    const tpl = $('#tpl-wall');
    const brokenTpl = $('#tpl-wall-broken');

    wallList.textContent = '';
    for (const it of items) {
      const frag = (it.broken ? brokenTpl : tpl).content.cloneNode(true);
      const li = frag.querySelector('li');
      $('.t', li).textContent = it.name;
      /* The row clips at the measure, so the full name has to stay reachable.
         CSS truncation does not touch the accessible name, so the text node
         is still complete for a screen reader; this is for the pointer. */
      li.title = it.raw;
      if (it.broken) {
        const btn = $('[data-delete]', li);
        btn.dataset.delete = it.path;
        btn.setAttribute('aria-label', `Delete ${it.name} from your watch`);
      } else {
        $('.s', li).textContent = it.note || '';
        $('.z', li).textContent = fmt(it.bytes);
      }
      wallList.appendChild(frag);
    }

    const brokenCount = items.filter((i) => i.broken).length;
    /* Count only what the device reported. A journal row is Pelican's own
       record of a send, not an observation of the watch, and counting the
       two together answers "what is on my watch" with a number the watch did
       not give. */
    const onWatch = items.filter((i) => !i.broken && i.onDevice).length;
    wallCount.textContent = brokenCount
      ? `${plural(onWatch, 'track')} aboard · ${brokenCount} unreadable`
      : `${plural(onWatch, 'track')} aboard`;

    /* One scroll region does the work. A "Show all 20" link under a list that
       is already cut off by its own scroll promises a reveal that scrolling
       has already given. */
    if (wall) wall.classList.toggle('is-empty', state.connected && items.length === 0);

    if (state.total > 0) {
      setLevel(water, (state.total - state.free) / state.total * 100);
      const free = fmtParts(state.free);
      const big = $('[data-cap-big]');
      big.textContent = '';
      big.append(
        document.createTextNode(free.n + ' '),
        Object.assign(document.createElement('span'),
          { className: 'capacity__unit', textContent: `${free.unit} free` }));
      /* No "N more hours will fit". 192 kbps is the *conversion* target only;
         MP3, M4A, M4B, AAC and WAV are copied byte-for-byte and keep their own
         rate, so the figure could overstate a WAV library sevenfold. The free
         figure above it is the honest answer and needs no companion. */
      $('[data-cap-sm]').textContent = `of ${fmt(state.total)}`;
    }
  }

  const stripExt = (n) => { const i = n.lastIndexOf('.'); return i <= 0 ? n : n.slice(0, i); };

  /* ── playback ───────────────────────────────────────────────────────── */

  const setIcon = (btn, name) => {
    const use = btn.querySelector('use');
    if (use) use.setAttribute('href', '#i-' + name);
  };

  function setPlaying(btn, playing, label) {
    btn.setAttribute('aria-pressed', String(playing));
    btn.setAttribute('aria-label', (playing ? 'Pause' : 'Play') + (label ? ' ' + label : ''));
    setIcon(btn, playing ? 'pause' : 'play');
  }

  let current = null;   /* the <tr> whose track is loaded */

  function clearRowStates() {
    $$('.tracks__play .iconbtn').forEach((other) => {
      const r = other.closest('tr');
      r.classList.remove('is-playing');
      setPlaying(other, false, $('.t', r)?.textContent.trim() || '');
    });
  }

  function wirePlay() {
    $$('.tracks__play .iconbtn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const row = btn.closest('tr');
        if (current === row && !audio.paused) { audio.pause(); return; }
        if (current === row && audio.src) { audio.play().catch(reportPlayFailure); return; }
        load(row);
      });
    });
  }

  async function load(row) {
    try {
      /* The URL carries the true on-disk path. WKWebView picks its decoder
         from the extension, so an opaque id would break FLAC and WAV even
         though the bytes are identical. */
      const url = await invoke('set_now_playing', { path: row.dataset.path });
      current = row;
      audio.src = url;
      /* Deliberately not gated on canPlayType(): it returns "" for FLAC,
         Ogg Opus and M4A on this WebKit while all three decode fine.
         Feature-detection here would reject most of a real library. */
      await audio.play();
    } catch (e) {
      /* `current` is only assigned once the invoke resolves, so on a rejected
         one it still points at the previously played row. Name the file that
         was actually clicked. */
      reportPlayFailure(e, row);
    }
  }

  function reportPlayFailure(e, row = current) {
    const why = audio.error ? `the decoder refused it (code ${audio.error.code})` : String(e);
    showError('Could not play that file', `${row ? row.dataset.name + ': ' : ''}${why}`);
  }

  function syncPlayerChrome() {
    const playing = !audio.paused && !audio.ended && audio.src;
    clearRowStates();
    if (current && playing) {
      current.classList.add('is-playing');
      const btn = $('.tracks__play .iconbtn', current);
      setPlaying(btn, true, $('.t', current).textContent.trim());
    }
    if (playpause) setPlaying(playpause, !!playing);
    if (current) {
      $('.player__meta .t').textContent = $('.t', current).textContent;
      $('.player__meta .s').textContent = $('.tracks__title .s', current).textContent;
      /* This is what the *file* is, read by lofty from the source on this
         Mac. Pelican never asks CoreAudio what the output device is doing
         with it, so the string must not be read as a claim about what is
         coming out of the speakers. */
      const f = $('.player__fmt');
      f.textContent = current.dataset.fmt || '';
      f.title = 'The format of the file on this Mac. Pelican does not know what your output device does with it.';
      f.setAttribute('aria-label', 'Source file format: ' + (current.dataset.fmt || 'unknown'));
    }
  }

  audio.addEventListener('play', syncPlayerChrome);
  audio.addEventListener('pause', syncPlayerChrome);
  audio.addEventListener('ended', () => { syncPlayerChrome(); });
  audio.addEventListener('error', () => { if (audio.src) reportPlayFailure(new Error('load failed')); });

  audio.addEventListener('loadedmetadata', () => {
    const d = Number.isFinite(audio.duration) ? audio.duration : 0;
    seek.max = String(d || 0);
    $('[data-time-total]').textContent = mmss(d);
  });

  audio.addEventListener('timeupdate', () => {
    if (seeking) return;
    seek.value = String(audio.currentTime);
    $('[data-time-now]').textContent = mmss(audio.currentTime);
    paintSeek();
  });

  if (playpause) {
    playpause.addEventListener('click', () => {
      if (!audio.src) return;
      if (audio.paused) audio.play().catch(reportPlayFailure); else audio.pause();
    });
  }

  /* Seek: keep the filled track in step with the thumb. */
  let seeking = false;
  function paintSeek() {
    const max = Number(seek.max) || 0;
    const pct = max > 0 ? (Number(seek.value) / max) * 100 : 0;
    seek.style.background =
      `linear-gradient(90deg, var(--ink-dim) ${pct}%, rgba(226,236,247,.10) ${pct}%)`;
  }
  if (seek) {
    seek.addEventListener('input', () => {
      seeking = true;
      $('[data-time-now]').textContent = mmss(Number(seek.value));
      paintSeek();
    });
    seek.addEventListener('change', () => {
      seeking = false;
      if (audio.src) audio.currentTime = Number(seek.value);
    });
    paintSeek();
  }

  /* ── transfer ───────────────────────────────────────────────────────── */

  /* The progress bar reported a static 43% to assistive tech for the whole
     transfer. Keep the value and the bar in step from one place. */
  function setProgress(pct) {
    if (!meter) return;
    meter.setAttribute('aria-valuenow', String(Math.round(pct)));
    const fill = meter.querySelector('i');
    if (fill) fill.style.transform = `scaleX(${(pct / 100).toFixed(3)})`;
  }
  setProgress(0);

  const notices = () => $('[data-notices]');

  function addNotice(tag, body) {
    const frag = $('#tpl-notice').content.cloneNode(true);
    $('.notice__tag', frag).textContent = tag;
    /* Verbatim. The engine's message is the only place that says whether a
       failed write left a stub behind, and therefore whether retrying is
       safe. Summarising it would throw that away. */
    $('.notice__body', frag).textContent = body;
    notices().appendChild(frag);
  }

  const SKIP_LABEL = {
    notAudio: 'Not audio', notPlayable: 'Cannot play as-is',
    missingTags: 'Needs a title', other: 'Skipped',
  };
  const FAIL_LABEL = {
    deviceBusy: 'Watch is held by something else', openSession: 'Could not open the watch',
    noEncoder: 'No converter for this format', sizeMismatch: 'Wrote the wrong size',
    other: 'Failed',
  };

  let run = null;   /* { total, bytes, startedAt, doneBytes } */

  function startRun(total, bytes) {
    run = { total, bytes, startedAt: Date.now(), jobBytes: 0 };
    state.syncing = true;
    notices().textContent = '';
    transfer.hidden = false;
    $('[data-transfer-total]').textContent = String(total);
    $('[data-transfer-done]').textContent = '0';
    $('[data-transfer-eta]').textContent = '';
    $('[data-now-label]').textContent = 'Preparing…';
    $('[data-now-bytes]').textContent = '';
    setProgress(0);
    updateSelection();
  }

  function paintRun(ev) {
    $('[data-transfer-done]').textContent = String(ev.completed);
    $('[data-transfer-total]').textContent = String(ev.total);
    /* Once there is a byte total, the byte-weighted figure is authoritative
       and monotone. Falling back to `completed / total` on the events that
       carry no `jobBytes` — fileStarted, fileStaging, fileDone — snapped the
       bar backwards every time a file finished. */
    if (run && run.bytes > 0) {
      if (typeof ev.jobBytes === 'number') run.jobBytes = Math.max(run.jobBytes, ev.jobBytes);
      setProgress(Math.min(100, run.jobBytes / run.bytes * 100));
    } else if (ev.total > 0) {
      setProgress(ev.completed / ev.total * 100);
    }
    /* Recomputed on every event, so a long staging phase cannot leave a
       stale estimate on screen. */
    $('[data-transfer-eta]').textContent = eta();
  }

  function eta() {
    if (!run || !run.jobBytes) return '';
    const elapsed = (Date.now() - run.startedAt) / 1000;
    if (elapsed < 3) return '';
    const rate = run.jobBytes / elapsed;
    if (rate <= 0) return '';
    const left = Math.round((run.bytes - run.jobBytes) / rate);
    if (left < 30) return ' · nearly done';
    if (left < 90) return ' · about a minute left';
    return ` · about ${Math.round(left / 60)} minutes left`;
  }

  /* ── the send gesture ───────────────────────────────────────────────── */

  /* Rows sink toward the water. The water itself is NOT moved here: it rises
     when the device reports its new free space, which is the only moment the
     claim is true. Honours prefers-reduced-motion by skipping the animation. */
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');

  function send() {
    const rows = checked();
    if (!rows.length) return;
    const paths = rows.map((r) => r.dataset.path);

    /* skip_tag_check is true on purpose, and the copy above depends on it:
       an untagged file must transfer and merely stay invisible. With it off
       the engine would silently skip those files and the warning would be a
       lie. */
    invoke('start_sync', { paths, skipTagCheck: true })
      .catch((e) => showError('Could not start the transfer', String(e)));

    const settle = () => {
      rows.forEach((r) => { $('input', r).checked = false; });
      updateSelection();
    };
    if (reduced.matches) { settle(); return; }
    rows.forEach((r) => r.classList.add('is-sending'));
    setTimeout(() => {
      rows.forEach((r) => r.classList.remove('is-sending'));
      settle();
    }, 260);
  }

  if (sendBtn) sendBtn.addEventListener('click', send);

  $('[data-stop]')?.addEventListener('click', (e) => {
    e.currentTarget.disabled = true;
    e.currentTarget.textContent = 'Stopping after this track…';
    invoke('stop_sync').catch(() => {});
  });

  /* ── errors ─────────────────────────────────────────────────────────── */

  function showError(tag, body, action) {
    if (!errBox) return;
    errBox.hidden = false;
    $('.prewarn__tag', errBox).textContent = tag;
    $('[data-error-body]').textContent = body;
    const act = $('[data-error-act]');
    act.hidden = !action;
    if (action) { act.textContent = action.label; act.onclick = action.run; }
  }
  const clearError = () => { if (errBox) errBox.hidden = true; };

  /* ── device state ───────────────────────────────────────────────────── */

  function applyState() {
    const connected = state.connected;
    app.dataset.device = connected ? 'connected' : 'none';

    /* Set the status in the DOM rather than through CSS content: generated
       text is not reliably announced, and this string is the answer to the
       first question a user asks. */
    /* `Attached` is the coarse pre-session state — USB enumeration only, and
       this watch publishes no product string there — so the header honestly
       reads "Garmin watch" for the second between attach and open, then
       sharpens to the model the MTP session reports. */
    if (deviceName) {
      deviceName.textContent = state.model || state.attached?.product || 'Garmin watch';
    }
    /* Contention is a completed, failed open just as much as any other
       failure — the titlebar used to keep saying "opening…" for as long as
       the other process held the device, three inches from a panel that said
       the watch was held. */
    if (status) {
      status.textContent = connected ? 'connected'
        : state.contention ? 'found — held by something else'
        : state.openFailed ? 'found — would not open'
        : state.attached ? 'found — opening…'
        : 'no watch found';
    }

    /* Local browsing and playback keep working with no watch: nothing in the
       scan or player path touches the device. */
    const contention = $('[data-contention]');
    if (contention) {
      contention.hidden = !state.contention;
      /* No separator: in this state the body below is empty, so the engine's
         sentence is the only sentence and stands directly under the heading. */
      contention.textContent = state.contention || '';
    }
    const emptyH = $('[data-empty-h]');
    const emptyBody = $('[data-empty-body]');
    if (emptyH) {
      emptyH.textContent = state.contention ? 'Your watch is held by something else'
        : state.openFailed ? 'Pelican could not open your watch'
        : connected ? 'Nothing on your watch yet'
        : state.attached ? 'Opening your watch…'
        : 'No watch connected';
    }
    /* The connected-and-empty case is what a new watch, or one the user has
       just cleared, looks like. It used to render a heading, "0 tracks
       aboard" and silence. */
    /* Contention gets no body of its own. The watch is plugged in and was
       found, so the disconnected copy ("plug your Garmin in") contradicted the
       heading above it and buried the one actionable line — the engine's own
       contention string, rendered just below — behind that contradiction. */
    if (emptyBody) {
      emptyBody.textContent = state.contention
        ? ''
        : state.openFailed
          ? state.openFailed
          : connected
            ? 'Tick a track on the left and press Send. It arrives as a file in /Music.'
            : 'Plug your Garmin in over USB. Pelican will find it on its own — there is no '
              + 'account to sign into and nothing to install on the watch.';
    }
    updateSelection();
  }

  /* ── the event stream ───────────────────────────────────────────────── */

  function onEvent(ev) {
    switch (ev.type) {
      case 'attached':
        state.attached = { label: ev.label, serial: ev.serial, product: ev.product };
        state.contention = null;
        state.openFailed = null;
        applyState();
        /* Auto-connect: the product's promise is that it finds the watch on
           its own. Policy lives here, not in the engine. */
        invoke('connect', { serial: ev.serial }).catch(() => {});
        break;

      case 'detached':
        state.attached = null;
        state.connected = false;
        state.model = null;
        state.openFailed = null;
        state.entries = []; state.uploads = [];
        state.free = 0; state.total = 0;
        applyState();
        renderWall();
        break;

      case 'snapshot':
        state.connected = true;
        state.contention = null;
        state.openFailed = null;
        state.model = ev.model || null;
        state.free = ev.free; state.total = ev.total;
        state.entries = ev.entries; state.uploads = ev.uploads;
        applyState();
        renderWall();
        break;

      case 'scanned':
        state.root = ev.root;
        state.tracks = ev.tracks;
        state.encoder = ev.encoder;
        state.encoderVerified = ev.encoderVerified;
        $('[data-count-all]').textContent = String(ev.found);
        renderHero();
        renderTracks(ev.tracks);
        renderEncoderNote();
        if (ev.truncated) {
          /* The omitted files are the tail of a path-sorted list, so opening
             a *different* folder cannot show them — opening one of this
             folder's subfolders can. */
          showError('Showing part of this folder',
            `It holds ${ev.found} audio files and Pelican is showing the first ` +
            `${ev.tracks.length} in path order. Open one of its subfolders to reach the others.`);
        }
        break;

      case 'planned':
        startRun(ev.total, ev.bytes);
        break;

      case 'fileStarted':
        paintRun(ev);
        $('[data-now-label]').textContent = ev.name;
        $('[data-now-bytes]').textContent = '';
        break;

      case 'fileStaging':
        paintRun(ev);
        /* The device is idle here. Saying "uploading" during a 40-second
           FLAC decode makes a working app look wedged. */
        $('[data-now-label]').textContent = `${ev.name} — converting on this Mac`;
        break;

      case 'fileProgress':
        paintRun(ev);
        $('[data-now-label]').textContent = `${ev.name} — sending`;
        $('[data-now-bytes]').textContent = `${fmt(ev.fileBytes)} of ${fmt(ev.fileTotal)}`;
        break;

      case 'fileDone':
        paintRun(ev);
        $('[data-now-bytes]').textContent = fmt(ev.bytes);
        break;

      case 'fileSkipped':
        paintRun(ev);
        addNotice(SKIP_LABEL[ev.kind] || SKIP_LABEL.other, `${ev.name}: ${ev.reason}`);
        break;

      case 'fileFailed':
        paintRun(ev);
        addNotice(FAIL_LABEL[ev.kind] || FAIL_LABEL.other, `${ev.name}: ${ev.error}`);
        if (ev.kind === 'deviceBusy' && ev.completed === 1) {
          /* Contention on the very first file almost always means a session
             leaked. Dropping it and opening a fresh one is the fix. */
          showError('The watch would not open', ev.error, {
            label: 'Reconnect',
            run: () => invoke('disconnect')
              .then(() => invoke('connect', { serial: state.attached?.serial }))
              .then(clearError)
              .catch((e) => showError('Reconnect failed', String(e))),
          });
        }
        break;

      case 'syncFinished': {
        state.syncing = false;
        const stop = $('[data-stop]');
        stop.disabled = false;
        stop.textContent = 'Stop after this track';
        $('[data-now-label]').textContent = ev.stopped
          ? `Stopped after ${plural(ev.ok, 'track')}.`
          : `Sent ${plural(ev.ok, 'track')}.`;
        $('[data-now-bytes]').textContent = '';
        $('[data-transfer-eta]').textContent = '';
        setProgress(100);
        /* A run with failures stays on screen with its list. A clean one
           fades out through the existing @starting-style transition. */
        if (ev.failed === 0 && ev.skipped === 0) {
          setTimeout(() => { transfer.hidden = true; }, 1400);
        }
        updateSelection();
        break;
      }

      case 'error':
        if (ev.contention) {
          state.connected = false;
          state.contention = ev.contention;
          applyState();
        } else if (state.syncing) {
          addNotice('Problem', ev.message);
        } else {
          showError('Pelican could not do that', ev.message);
          /* An open that failed for any reason other than contention left
             the UI claiming an open was still in flight. */
          if (!state.connected && state.attached) {
            state.openFailed = ev.message;
            applyState();
          }
        }
        break;
    }
  }

  /* ── wiring ─────────────────────────────────────────────────────────── */

  $('[data-pick]')?.addEventListener('click', (e) => {
    e.preventDefault();
    clearError();
    invoke('pick_folder')
      .then((path) => { if (path) return invoke('scan_folder', { path }); })
      .catch((err) => showError('Could not open that folder', String(err)));
  });

  wallList.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-delete]');
    if (!btn) return;
    btn.disabled = true;
    invoke('delete_remote', { path: btn.dataset.delete })
      .catch((err) => { btn.disabled = false; showError('Could not delete that', String(err)); });
  });

  /* ── startup ────────────────────────────────────────────────────────── */

  if (NATIVE) {
    const ch = new core.Channel();
    ch.onmessage = onEvent;
    invoke('subscribe', { onEvent: ch })
      .catch((e) => showError('Pelican could not start', String(e)));
  } else {
    /* Explicit dev override, so the states stay reviewable in a plain browser
       with no watch and no build. Never reached inside the app, because
       NATIVE is always true there.

       `onEvent` is exposed here and only here: the data-driven states — a
       scanned folder, a populated wall, a run with failures — cannot be
       reviewed from the hash alone, and a reviewer without a Forerunner on
       the desk should still be able to see them. Feed it the same event
       shapes the engine emits. */
    window.__pelicanDevEvent = onEvent;
    const applyHash = () => {
      const s = location.hash.replace('#', '');
      state.connected = s !== 'disconnected';
      /* A synthetic serial. The fallback only needs a serial-shaped string to
         exercise the layout; a real device's is nobody's business in a build. */
      state.attached = state.connected ? { label: 'Garmin device', serial: '0000000000' } : null;
      /* The model comes off the MTP session in the app, so the fallback puts
         it where the app puts it. */
      state.model = state.connected ? 'Forerunner 165 Music' : null;
      state.total = 3.71e9; state.free = 2.41e9;
      applyState();
      renderWall();
      if (transfer) transfer.hidden = s !== 'transferring';
    };
    window.addEventListener('hashchange', applyHash);
    applyHash();
  }

  applyState();
  renderHero();
  updateSelection();
  /* Nothing is loaded yet, so the transport is a Play control. Left to the
     markup alone it shipped showing Pause, which told both the eye and a
     screen reader that something was playing. */
  if (playpause) setPlaying(playpause, false);
})();
