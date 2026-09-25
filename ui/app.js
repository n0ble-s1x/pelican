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
  const gauge = $('.gauge');
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
  const wallbar = $('[data-wallbar]');
  const confirmBox = $('[data-confirm]');
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
    /* A delete in flight, and the free reading taken before it went out.
       `Deleted` carries the tallies and the snapshot that follows carries the
       new free figure, so the report needs both events and this is where the
       first half waits. Null whenever nothing is pending. */
    pendingDelete: null,
    entries: [], uploads: [],
    tracks: [], root: null,
    /* Which question the library is answering, and whether one answer is
       open. Grouping is pure JS over `tracks`, computed on demand: no new
       parse, no new IPC, no persistence, and emphatically no network. */
    view: 'all',            /* 'all' | 'albums' | 'artists' */
    focus: null,            /* null, or { kind:'albums'|'artists', key } */
    encoder: null, encoderVerified: false,
    contention: null,
    /* The run, in `state` like everything else.
       `transfer.hidden` used to be the one piece of UI set imperatively while
       every other pixel was painted from here, and the two disagreed in a
       reachable sequence: pull the cable mid-send and `detached` sets
       data-device="none" while the card is still displayed over the channel,
       so "Connect your watch to send music." painted underneath a card that
       claimed a live transfer. Deriving both from one object is the fix;
       hiding one of them would have been a patch over the same hole.
         null, or
         { phase:'running'|'finished', total, bytes, jobBytes, startedAt,
           ok, skipped, failed, stopped, delivered, planned,
           landed: [], dismissed: false } */
    run: null,
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

  /* One custom property on the gauge, and the stylesheet does the rest: the
     water translates by it and the pending band sits on it. It used to be an
     inline `transform` here and an inline `bottom` there, two mechanisms for
     one number that could only ever disagree.
     A CSSOM write, not a style attribute — the CSP is style-src 'self' with
     no 'unsafe-inline', which drops attributes in markup and does not touch
     this. */
  const setLevel = (pct) => {
    if (gauge) gauge.style.setProperty('--used', pct.toFixed(2) + '%');
  };

  /* ── selection ──────────────────────────────────────────────────────── */

  /* The library's selection, keyed on path and kept out of the DOM.

     It used to live in the checkboxes themselves, and `renderTracks` empties
     the tbody and clones fresh unticked rows — so every rail click, every
     drill into an album and every crumb-back destroyed it. Ticking five
     tracks in one album and five in another was impossible by construction,
     because `send()` read the DOM and the DOM only ever held the page in
     front of you. The wall next door already does this correctly with
     `wallPicks`/`wallGhosts`; this is the same pattern on the panel that
     actually has the rows.

     Pruned in the `scanned` handler against the new track list, the way
     `renderWall` prunes against the live listing: a re-scan must not leave a
     path in a set that Send would then act on. */
  const libPicks = new Set();

  /* Selected tracks as the scan described them, not as the current view
     happens to be rendering. Everything that counts, sums or names the
     selection reads this, so those figures are right across views instead of
     describing only the visible page. */
  const checked = () => state.tracks.filter((t) => libPicks.has(t.path));

  /* The subset that is on screen right now. Only the send animation wants
     this — it moves rows, and a row that is not rendered cannot move. */
  const checkedRows = () =>
    $$('.tracks tbody tr').filter((tr) => libPicks.has(tr.dataset.path));

  /* What the selection is actually missing, in four separate buckets,
     because they are four different things to tell someone.

     This was one boolean — `playableInLibrary`, which is
     `title.is_some() && artist.is_some()` — and the screen expanded that one
     bit back into two specific claims: a heading saying tracks "need a
     title" and a body saying named files "have no title or artist". Both
     were false about any file that had a title and no artist, and both were
     false about a file Pelican could not open at all, which arrived here as
     the same `false`. The owner reported it as the app lying about his own
     music, and he was right.

     A file Pelican could not parse is in its own bucket and gets its own
     sentence. Pelican has no evidence about that file's tags and may not
     make a claim about them. */
  const gapOf = (t) => {
    if (!t.readable) return 'unreadable';
    if (!t.hasTitle && !t.hasArtist) return 'both';
    if (!t.hasTitle) return 'title';
    if (!t.hasArtist) return 'artist';
    return null;
  };

  const gaps = () => {
    const out = { unreadable: [], both: [], title: [], artist: [] };
    for (const t of checked()) {
      const g = gapOf(t);
      if (g) out[g].push(t);
    }
    return out;
  };

  /* Identify a file by its title where it has one, and by its filename where
     it does not — never by a filename inside a sentence about titles. The
     old copy named every file by `dataset.name`, so a titled file was
     identified by its filename in a sentence claiming it had no title. */
  const gapName = (t) => (t.hasTitle ? t.title : t.path.split('/').pop());

  /* One reading of the situation, for both the send button's reason and the
     transfer card's heading.

     They were composed independently and could contradict each other on
     screen: `updateSelection` ranked a `syncing` flag above the device check,
     but that flag cleared when the run finished while the card deliberately
     stayed up on failure — so a cable pull produced "Connect your watch to
     send music." at the top of the channel with a card above it still saying
     "Sending to your watch". Two sentences from one function cannot do that.

     `sel` carries the selection facts, which the heading has no opinion on. */
  function situation(sel) {
    const connected = state.connected;
    const r = state.run;
    const running = !!r && r.phase === 'running';

    let heading;
    if (!r) heading = 'Sending to your watch';
    else if (running) {
      /* The card must never claim a live transfer to a watch that is gone.
         The run is genuinely still draining — transfer::run continues past
         every failure — so this says what is true of both. */
      heading = connected ? 'Sending to your watch'
                          : 'Sending — your watch is no longer connected';
    } else heading = outcomeHeading(r);

    let why = '';
    let blocked = 'reason';
    /* With no folder open there is nothing tickable, the hero already says
       so, and a red line under it would be the app alarming about its own
       first-run state. */
    if (!state.tracks.length) why = '';
    else if (running) why = 'A transfer is already running.';
    else if (!connected) why = 'Connect your watch to send music.';
    else if (!sel.rows.length) why = 'Tick a track to send it.';
    else if (!sel.fits) {
      why = `That is ${sel.about}${fmt(sel.bytes - state.free)} more than your watch has room for.`;
      blocked = 'capacity';
    }
    /* The send is possible — which is the moment this belongs, and the only
       moment. It is a warning a person reads while deciding, so it sits on
       the button it describes (aria-describedby="send-why") rather than in a
       notice they would have to go and find.

       `docs/garmin-mtp.md` §8: on 2026-09-05, FR165 FW 2506, 22 files were
       deleted, every DeleteObject returned Ok, /Music fell to one entry and
       free space rose 78.5 MB — and the watch's music app still listed all
       22. So the sentence has to hold two things at once. It must not say
       Pelican can take a track off the watch, because there is no evidence it
       can. And it must not say Pelican cannot, because the mechanism is not
       established and one observation on one firmware is not a property of
       the device. Hence "may", "the one time", "we don't know why yet" —
       each hedge is carrying a specific piece of missing evidence, not
       softening the warning.

       Ink is --ink-dim on --air, 6.38:1 (app.css, .sendwhy). Signal Red is
       reserved for the over-capacity case by DESIGN.md's exhaustive list, so
       `blocked` stays 'reason': this is a fact about the watch, not a
       failure of the selection. */
    else {
      why = 'Sending may be one way. Pelican can delete the file later, but '
          + 'the one time that was tried the watch’s music app still listed '
          + 'the tracks — we don’t know why yet.';
    }
    return { heading, why, blocked, running, cardUp: !!r && !r.dismissed };
  }

  /* The terminal sentence. Past tense, and it names the outcome rather than
     leaving a present-tense "Sending to your watch" over a finished run. */
  function outcomeHeading(r) {
    const base = terminalSentence(r);
    const n = r.landed ? r.landed.length : 0;
    if (!n) return base;
    /* Reconciliation revises this sentence at its source, so every reading of
       the card is revised at once: the heading `applyState` paints, the
       `aria-valuetext` the meter announces, and the now-line. A correction
       written into only one of those is the same wrong sentence, moved from
       the eye to the ear. */
    return `${base} — but ${n === 1 ? 'one of those is' : n + ' of those are'} `
         + 'on your watch';
  }

  function terminalSentence(r) {
    if (r.stopped) return `Stopped after ${plural(r.ok, 'track')}`;
    const parts = [];
    if (r.ok) parts.push(`${r.ok} sent`);
    if (r.skipped) parts.push(`${r.skipped} skipped`);
    if (r.failed) parts.push(`${r.failed} failed`);
    if (!r.failed && !r.skipped) return `Sent ${plural(r.ok, 'track')}`;
    return `Send finished — ${parts.length ? parts.join(', ') : 'nothing to send'}`;
  }

  function updateSelection() {
    const rows = checked();
    const bytes = rows.reduce((n, t) => n + Number(t.bytes || 0), 0);
    const known = state.total > 0;
    const fits = !known || bytes <= state.free;
    /* Anything that has to be converted has a *modelled* size — duration at
       the target bitrate — not a measured one, and scan.rs says so in its own
       doc comment. Two significant figures on a projection reads as a
       measurement, so the projection says it is one. */
    const est = rows.some((t) => t.pipeline && t.pipeline !== 'passthrough');
    const about = est ? 'about ' : '';

    /* The band sits on top of the current level and is measured against
       total capacity, so it answers "will this fit" directly. The percentage
       goes into --pending rather than straight onto `height`, so the
       stylesheet's `max(2px, …)` floor can do its job: in a 132px gauge a
       3% selection is 4px, and a real selection must not round to nothing. */
    if (pending && known) {
      const pct = Math.min(100, bytes / state.total * 100);
      pending.style.setProperty('--pending', pct.toFixed(2) + '%');
      pending.style.opacity = pct > 0 ? '1' : '0';
    } else if (pending) {
      pending.style.setProperty('--pending', '0%');
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

    const sit = situation({ rows, bytes, fits, about });

    if (sendBtn) {
      sendBtn.textContent = rows.length ? `Send ${plural(rows.length, 'track')}` : 'Send';
      sendBtn.disabled =
        !rows.length || !fits || app.dataset.device === 'none' || sit.running;
    }

    /* A disabled control that does not say why is a dead end. The reason is
       rendered in the DOM and referenced by aria-describedby, so it reaches
       pointer and screen reader alike. */
    if (sendWhy) {
      sendWhy.textContent = sit.why;
      /* Signal Red is spent on the over-capacity case only — the one reason
         on DESIGN.md's list for that ink. The rest are quiet statements of
         fact, not failures. */
      sendWhy.dataset.blocked = sit.blocked;
      sendWhy.hidden = !sit.why;
    }

    /* The card and the line above are the same sentence, painted together. */
    if (transfer) {
      transfer.hidden = !sit.cardUp;
      $('[data-transfer-h]').textContent = sit.heading;
    }

    if (prewarn) renderPrewarn();
    updateLibAll();
  }

  /* One sentence per bucket, and every sentence names only the field the
     scan actually found absent. The old copy asserted "no title or artist"
     over a single boolean; this one can say "3 tracks have no artist" and be
     right about all three.

     Nothing here says the watch "will never" show the file, and nothing says
     "only". The reference is docs/garmin-mtp.md's Storage layout note, and
     that note is the one strong device claim in the file with no provenance
     stamp — no firmware, no date, no record of what was measured. §8 of the
     same file then showed the music app listing 22 entries whose objects were
     already deleted from /Music, so what the app lists is not a function of
     what /Music holds. "Lists only files carrying both" was therefore a rule
     the repo has not earned. What is said instead is the shape of the
     evidence: such a file has not been SEEN to appear. That is weaker, it is
     true, and it still tells the owner what to do about it. */
  function renderPrewarn() {
    const b = gaps();
    const tagged = b.both.length + b.title.length + b.artist.length;
    const total = tagged + b.unreadable.length;
    prewarn.hidden = total === 0;
    if (!total) return;

    const tag = [];
    const needs = (rows, what) => {
      if (!rows.length) return;
      tag.push(`${plural(rows.length, 'track')} ${rows.length === 1 ? 'needs' : 'need'} ${what}`);
    };
    needs(b.both, 'a title and an artist');
    needs(b.title, 'a title');
    needs(b.artist, 'an artist');
    if (b.unreadable.length) {
      tag.push(`${plural(b.unreadable.length, 'file')} could not be read`);
    }
    $('.prewarn__tag', prewarn).textContent = tag.join(' · ');

    const body = $('[data-prewarn-body]');
    body.textContent = '';
    /* A run of names in --ink, then the claim in --ink-dim. Both ratios are
       already measured against this ground in app.css. */
    const say = (rows, sentence) => {
      if (!rows.length) return;
      const strong = document.createElement('strong');
      strong.textContent = nameList(rows.map(gapName));
      if (body.childNodes.length) body.append(document.createTextNode(' '));
      body.append(strong, document.createTextNode(sentence(rows.length)));
    };
    say(b.both, (n) => (n === 1 ? ' has no title and no artist.' : ' have no title and no artist.'));
    say(b.title, (n) => (n === 1 ? ' has no title.' : ' have no title.'));
    say(b.artist, (n) => (n === 1 ? ' has no artist.' : ' have no artist.'));
    if (tagged) {
      body.append(document.createTextNode(
        (tagged === 1 ? ' It will copy' : ' They will copy')
        + ' across, but a file missing either tag has not been seen to appear'
        + " in the watch's music app."
        /* Names the remedy in prose. This is what the permanently disabled
           "Add tags…" button next to it was standing in for, and Pelican does
           not write to your source files — see docs/tag-editing-proposal.md.
           "Rescan" is the control now in the hero, so it is named. */
        + ' Add the missing tags in whatever tagger you use, then Rescan.'));
    }
    /* Deliberately no claim about this file's tags: the parser never got far
       enough to have one. Telling someone a truncated FLAC is "untagged"
       sends them to fix the wrong thing. */
    say(b.unreadable, (n) =>
      (n === 1 ? ' could not be read by Pelican' : ' could not be read by Pelican')
      + ' — the file may be truncated or corrupt, so nothing is known about'
      + (n === 1 ? ' its' : ' their') + ' tags.');
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
    /* Indexed because a refusal note below needs a unique id to be pointed at
       by `aria-describedby`, and a path cannot be one — paths carry spaces.
       The tbody is emptied on every render, so the ids never collide. */
    for (const [rowIndex, t] of tracks.entries()) {
      const frag = tpl.content.cloneNode(true);
      const tr = frag.querySelector('tr');
      const name = t.path.split('/').pop();

      tr.dataset.path = t.path;
      tr.dataset.name = name;
      tr.dataset.bytes = String(t.bytes);
      tr.dataset.fmt = t.fmt;
      tr.dataset.pipeline = t.pipeline;
      /* For MediaMetadata and for the album key the cover cache uses. The
         title falls back to the file stem in scan.rs, so it is never blank. */
      tr.dataset.title = t.title;
      tr.dataset.artist = t.artist || '';
      tr.dataset.album = t.album || '';
      /* Name the absence, do not flatten it. `unreadable` is first because it
         is not a statement about tags at all — see `gaps()`. */
      if (!t.readable) tr.dataset.gap = 'unreadable';
      else if (!t.hasTitle && !t.hasArtist) tr.dataset.gap = 'both';
      else if (!t.hasTitle) tr.dataset.gap = 'title';
      else if (!t.hasArtist) tr.dataset.gap = 'artist';

      const label = t.artist ? `${t.title} — ${t.artist}` : t.title;
      const titleEl = $('.t', tr);
      titleEl.textContent = t.title;
      /* scan.rs falls this column back to the file stem so a row is never
         blank, which is right — but a stem is a filename, not a title. The
         app used to print one here in --ink and then, four rows lower, tell
         the owner that same file had no title. Mark it so the column stops
         asserting something the file does not carry. */
      if (!t.hasTitle) titleEl.dataset.stem = 'true';
      /* The format column is a fixed 96px in the approved comp, so anything
         longer than a format name belongs on the subtitle line instead — it
         wraps to three lines and drags the whole row's height with it. */
      /* A duration of zero is what scan.rs writes when lofty could not open
         the file at all. Unknown and zero are different facts, so the
         unknown one is left unsaid rather than printed as `0:00`.

         And "no artist" is a claim about the file's tags, which Pelican is
         not entitled to make about a file it could not parse. Unreadable
         gets its own words here for the same reason it gets its own bucket
         in `gaps()`. */
      $('.tracks__title .s', tr).textContent =
        (t.readable
          ? (t.durationSecs ? `${mmss(t.durationSecs)} · ` : '') + (t.artist ? t.artist : 'no artist')
          : 'Pelican could not read this file')
        + (t.sendable && t.pipeline !== 'passthrough' ? ' · converts to 192 kbps' : '')
        + (t.sendable ? '' : ' · cannot be converted for your watch');
      $('.tracks__fmt', tr).textContent = t.fmt;

      const play = $('.tracks__play .iconbtn', tr);
      play.setAttribute('aria-label', 'Play ' + label);
      /* Ogg Vorbis is excluded rather than untested: the media element
         reports readyState 4, a correct duration and a moving clock, and
         emits digital silence. That failure is invisible from its own API,
         so it has to be refused here. */
      /* `aria-disabled`, not `disabled`. A `disabled` button is not
         focusable, so the one sentence explaining the refusal lived in a
         `title` and was reachable by pointer only — a keyboard or screen
         reader user got a dead control and no reason at all. The control
         stays focusable and describes itself; `wirePlay` refuses the click.
         Same argument the send button already makes with `#send-why`: a
         control that cannot be operated has to say why, to everyone. */
      if (OGG_SILENT.has(extOf(t.path))) {
        const why = 'Pelican cannot preview Ogg files on this Mac.';
        play.setAttribute('aria-disabled', 'true');
        play.title = why;
        const id = `why-ogg-${rowIndex}`;
        const note = document.createElement('span');
        note.className = 'sr-only';
        note.id = id;
        note.textContent = why;
        play.after(note);
        play.setAttribute('aria-describedby', id);
      }

      const pick = $('input', tr);
      pick.setAttribute('aria-label', 'Send ' + label);
      /* Painted from the set, so a selection made in one album is still there
         when you come back to it.

         No `.is-picked` row tint here, unlike the wall. The wall's rows carry
         --ink and --ink-dim only; a library row also carries the format
         column in --ink-faint at 10.5px, and --ink-faint on --surface-2 over
         --air (#1e2733) measures 4.38:1 — under AA. The tick is the state. */
      pick.checked = libPicks.has(t.path);
      /* The engine already wrote the honest sentence for both cases — the
         refusal and the conversion profile. Showing it only for refusals
         threw the conversion disclosure away. */
      if (t.note) pick.title = t.note;
      if (!t.sendable) {
        /* Same argument as the Ogg play button above, and the rule DESIGN.md
           states: a reason nobody can reach is not a reason. `disabled` takes
           the checkbox out of the tab order, so the engine's sentence — which
           names the format AND the fix, e.g. install ffmpeg for .opus — was
           reachable by hovering a mouse and by nothing else. aria-disabled
           keeps it focusable and described; the change handler refuses. */
        pick.setAttribute('aria-disabled', 'true');
        pick.setAttribute('aria-label', `Cannot send ${label}`);
        pick.closest('.pickhit')?.classList.add('is-off');
        if (t.note) {
          const id = `why-send-${rowIndex}`;
          const note = document.createElement('span');
          note.className = 'sr-only';
          note.id = id;
          note.textContent = t.note;
          pick.after(note);
          pick.setAttribute('aria-describedby', id);
        }
      }

      rowsEl.appendChild(frag);
    }

    shownTracks = tracks;
    wirePlay();
    buildQueue();
    publishMediaSession();
    updateSelection();
  }

  /* ── library bulk selection ─────────────────────────────────────────── */

  /* What `renderTracks` last painted. The select-all is scoped to this,
     because this is the scope the user can see — "all 47" must mean the 47
     in front of them, not 2000 across the folder. */
  let shownTracks = [];

  /* The anchor for a shift-range, as an index into the rendered rows. */
  let lastLibIndex = null;

  const libRows = () => $$('.tracks tbody tr');

  const setLibPick = (path, on) => {
    if (!path) return;
    if (on) libPicks.add(path); else libPicks.delete(path);
  };

  function updateLibAll() {
    const btn = $('[data-lib-all]');
    if (!btn) return;
    const sendable = shownTracks.filter((t) => t.sendable);
    btn.hidden = sendable.length === 0;
    if (!sendable.length) return;
    const all = sendable.every((t) => libPicks.has(t.path));
    btn.textContent = all ? 'Select none' : `Select all ${sendable.length}`;
    /* WCAG 2.5.3 Label in Name: aria-label overrides the visible words, so the
       name must CONTAIN them or a speech-input user saying what is printed on
       the button gets no match. The "all" branch already did — "Select all 22
       tracks in this view" opens with the visible "Select all 22". The "none"
       branch did not: "Clear the selection" is a description of the same act,
       not the label, and one state of a two-state control being unsayable is
       as broken as both. The visible label leads, the detail follows.
       Same shape on [data-wall-all] and [data-pick-stubs]. */
    btn.setAttribute('aria-label', all
      ? 'Select none — clear the selection'
      : `Select all ${plural(sendable.length, 'track')} in this view`);
    btn.dataset.all = all ? 'yes' : 'no';
  }

  $('[data-lib-all]')?.addEventListener('click', () => {
    const on = $('[data-lib-all]').dataset.all !== 'yes';
    /* Mutate the set and repaint once. Never by dispatching N synthetic
       change events: each one would re-enter the handler, and the same
       shortcut in the wall's stubs link is what made that gesture quadratic. */
    for (const t of shownTracks) if (t.sendable) setLibPick(t.path, on);
    for (const tr of libRows()) {
      const box = $('.tracks__pick input', tr);
      /* Checks aria-disabled as well as .disabled: unsendable rows moved to
         the ARIA form so their reason stays reachable, and a bulk tick must
         still skip them. */
      if (box && !box.disabled && box.getAttribute('aria-disabled') !== 'true') {
        box.checked = libPicks.has(tr.dataset.path);
      }
    }
    lastLibIndex = null;
    updateSelection();
  });

  /* One delegated pair on the tbody, wired once — `renderTracks` used to
     attach a fresh change listener to every checkbox on every render. */
  rowsEl.addEventListener('click', (e) => {
    const pick = e.target.closest('.tracks__pick input');
    if (!pick || !e.shiftKey || lastLibIndex === null) return;
    /* `click` fires after the checkbox's own state has flipped, so
       `pick.checked` is already the value the whole range should take. */
    const rows = libRows();
    const i = rows.indexOf(pick.closest('tr'));
    if (i < 0) return;
    const [a, b] = i < lastLibIndex ? [i, lastLibIndex] : [lastLibIndex, i];
    for (let k = a; k <= b; k++) {
      const box = $('.tracks__pick input', rows[k]);
      /* A file no encoder can read stays out of the range: its checkbox is
         disabled, and a bulk gesture must not reach past a refusal. */
      if (!box || box.disabled) continue;
      box.checked = pick.checked;
      setLibPick(rows[k].dataset.path, pick.checked);
    }
    updateSelection();
  });

  rowsEl.addEventListener('change', (e) => {
    const pick = e.target.closest('.tracks__pick input');
    if (!pick) return;
    /* aria-disabled keeps the control focusable so its reason can be read,
       which means the refusal has to happen here instead of in the DOM. */
    if (pick.getAttribute('aria-disabled') === 'true') { pick.checked = false; return; }
    const tr = pick.closest('tr');
    setLibPick(tr.dataset.path, pick.checked);
    lastLibIndex = libRows().indexOf(tr);
    updateSelection();
  });

  /* ── library grouping ───────────────────────────────────────────────── */

  const parentDir = (p) => {
    const parts = p.split('/').filter(Boolean);
    return parts.length > 1 ? parts[parts.length - 2] : '';
  };

  /* `artist` arrives already resolved album-artist-first (tags.rs:41-46,
     deliberately, so a soundtrack with per-track composer credits does not
     fragment into several albums). Nothing here re-derives it. */
  function libraryGroups(kind) {
    const map = new Map();
    for (const t of state.tracks) {
      const artist = (t.artist || '').trim();
      if (kind === 'artists') {
        const key = artist.toLowerCase();
        if (!map.has(key)) {
          map.set(key, {
            key,
            /* Never silently merged into someone else's rows, and never
               labelled with a name the file does not carry. */
            label: artist || 'No artist tag',
            byFolder: false,
            artist,
            tracks: [],
          });
        }
        map.get(key).tracks.push(t);
        continue;
      }
      const album = (t.album || '').trim();
      /* No album tag: group by the folder the files are actually in, which
         is free and is what a folder of untagged rips genuinely is — and say
         so in the header rather than inventing an album name. */
      const folder = parentDir(t.path);
      const label = album || folder || 'No album tag';
      const key = album
        ? `a\u0000${album.toLowerCase()}\u0000${artist.toLowerCase()}`
        : `f\u0000${folder.toLowerCase()}`;
      if (!map.has(key)) {
        map.set(key, { key, label, artist, byFolder: !album, date: t.date || '', tracks: [] });
      }
      map.get(key).tracks.push(t);
    }
    /* Explicit tuple comparison, not a joined sort key: a NUL separator is
       ignored by `localeCompare`, which silently collapsed the tuple and
       sorted an untagged group as if its artist were its album's first
       letter. Albums go (artist, date, album); an unknown artist sorts last,
       which is where a "we do not know" bucket belongs. */
    const cmp = (x, y) =>
      String(x || '').toLowerCase().localeCompare(String(y || '').toLowerCase());
    const groups = [...map.values()].sort((a, b) => {
      if (kind === 'artists') {
        return cmp(a.artist || '\uffff', b.artist || '\uffff');
      }
      return cmp(a.artist || '\uffff', b.artist || '\uffff')
        || cmp((a.date || '').slice(0, 4), (b.date || '').slice(0, 4))
        || cmp(a.label, b.label);
    });
    /* disc, then track, then path. `path` is the stable tiebreak, matching
       scan.rs's own "arbitrary but stable" discipline — and a missing track
       number sorts last rather than as zero, because absent and 0 are
       different facts. */
    for (const g of groups) g.tracks.sort(trackOrder);
    return groups;
  }

  function trackOrder(a, b) {
    const disc = (t) => Number(String(t.disc || '1').split('/')[0]) || 1;
    if (disc(a) !== disc(b)) return disc(a) - disc(b);
    const n = (t) => (typeof t.track === 'number' ? t.track : Number.MAX_SAFE_INTEGER);
    if (n(a) !== n(b)) return n(a) - n(b);
    return a.path.localeCompare(b.path);
  }

  function currentGroups() {
    return state.view === 'all' ? [] : libraryGroups(state.view);
  }

  function renderGroups(groups) {
    const list = $('[data-groups]');
    const tpl = $('#tpl-group');
    list.textContent = '';
    for (const g of groups) {
      const frag = tpl.content.cloneNode(true);
      const btn = frag.querySelector('.group');
      btn.dataset.key = g.key;
      $('.t', btn).textContent = g.label;
      const mins = Math.round(g.tracks.reduce((n, t) => n + (t.durationSecs || 0), 0) / 60);
      if (state.view === 'albums') {
        $('.s', btn).textContent =
          [g.artist || null, g.byFolder ? 'grouped by folder — no album tag' : null]
            .filter(Boolean).join(' · ') || 'No artist tag';
      } else {
        /* Count only albums the files actually name. An untagged track has no
           album, and counting its absence as "1 album" would report a fact
           the library does not hold. */
        const albums = new Set(
          g.tracks.map((t) => (t.album || '').trim().toLowerCase()).filter(Boolean));
        $('.s', btn).textContent = albums.size
          ? plural(albums.size, 'album')
          : 'no album tags';
      }
      $('.group__n', btn).textContent =
        `${plural(g.tracks.length, 'track')}${mins ? ` · ${mins} min` : ''}`;
      btn.addEventListener('click', () => {
        state.focus = { kind: state.view, key: g.key };
        applyLibraryView();
      });
      list.appendChild(frag);
    }
  }

  /* Owns everything that differs between the three views, so no caller has to
     remember which of the table, the group list and the crumb belongs to
     which. `renderTracks`'s row contract is untouched — same template, same
     data-*, same checkbox and send path — so selection, the waterline and the
     transfer need no knowledge of any of this. */
  function applyLibraryView() {
    app.dataset.view = state.view;
    $$('[data-view]').forEach((a) => {
      const on = a.dataset.view === state.view;
      a.classList.toggle('is-current', on);
      if (on) a.setAttribute('aria-current', 'page');
      else a.removeAttribute('aria-current');
    });

    const groups = currentGroups();
    const focused = state.focus && state.focus.kind === state.view
      ? groups.find((g) => g.key === state.focus.key)
      : null;
    if (state.focus && !focused) state.focus = null;

    const showGroups = state.view !== 'all' && !focused;
    $('[data-groups]').hidden = !showGroups;
    $('[data-tracks]').hidden = showGroups;

    const crumb = $('[data-crumb]');
    crumb.hidden = !focused;
    if (focused) {
      $('[data-crumb-back]').textContent =
        state.view === 'albums' ? '← All albums' : '← All artists';
    }

    if (showGroups) {
      renderGroups(groups);
      renderTracks([]);
    } else {
      renderTracks(focused ? focused.tracks : state.tracks);
    }
    renderHero(focused);
  }

  $$('.rail [data-view]').forEach((a) => {
    a.addEventListener('click', (e) => {
      e.preventDefault();
      state.view = a.dataset.view;
      state.focus = null;
      applyLibraryView();
    });
  });
  $('[data-crumb-back]')?.addEventListener('click', () => {
    state.focus = null;
    applyLibraryView();
  });

  function renderHero(focused) {
    const title = $('[data-hero-title]');
    const by = $('[data-hero-by]');
    const facts = $('[data-hero-facts]');

    /* The empty state's affordance and, once a folder is open, the way to
       re-read it — one control, because they are the same gesture pointed at
       a path Pelican either has or is about to ask for. */
    const act = $('[data-folder-act]');
    if (act) {
      /* Label in Name again (2.5.3). The test is containment, not overlap:
         the name "Read this folder again" reuses two of the button's words
         and still does not CONTAIN "Rescan this folder", so a speech-input
         user saying what is printed gets no match. The visible words lead
         and the qualifier follows. The empty state carries no aria-label at
         all — its own text is already the whole label, and the trailing
         ellipsis is the standard "this opens a dialog" mark. Removing rather
         than rewriting it also means the attribute cannot survive the
         transition back and leave a name from the other state. */
      if (!state.root) {
        act.textContent = 'Choose a folder of music…';
        act.removeAttribute('aria-label');
      } else {
        act.textContent = 'Rescan this folder';
        act.setAttribute('aria-label', 'Rescan this folder — read it again from disk');
      }
    }

    if (!state.root) {
      title.textContent = 'No folder open';
      by.textContent = 'Choose a folder of music to get started.';
      facts.textContent = '';
      return;
    }
    const scope = focused ? focused.tracks : state.tracks;
    title.textContent = focused
      ? focused.label
      : (state.root.split('/').filter(Boolean).pop() || state.root);

    const artists = [...new Set(scope.map((t) => t.artist).filter(Boolean))];
    by.textContent = artists.length === 0 ? 'No artist tags in this folder'
      : artists.length <= 3 ? artists.join(', ')
      : `${artists.slice(0, 2).join(', ')} and ${artists.length - 2} more`;

    /* A folder-grouped album is not an album Pelican was told about, and the
       hero is where that has to be said — the alternative is a heading that
       looks like a tag the files do not carry. */
    if (focused && focused.byFolder) {
      by.textContent = (by.textContent === 'No artist tags in this folder' ? '' : by.textContent + ' · ')
        + 'grouped by folder — these files carry no album tag';
    }

    const secs = scope.reduce((s, t) => s + (t.durationSecs || 0), 0);
    const hrs = Math.floor(secs / 3600);
    const mins = Math.round((secs % 3600) / 60);
    const fmts = [...new Set(scope.map((t) => t.fmt.split(' ')[0]))];
    facts.textContent = [
      plural(scope.length, 'track'),
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
    /* `encoderVerified` is always false today: nothing Pelican produces has
       been confirmed to play on a watch. This branch is therefore dead, and
       kept only so a real confirmation has somewhere to land. Whatever goes
       here must name the model and firmware the track was *heard* on. */
    if (state.encoderVerified) {
      tag.textContent = `Converting with ${state.encoder}`;
      body.textContent =
        `Files your watch cannot play will be converted on this Mac with ${state.encoder}.` +
        (refused ? ` ${plural(refused, 'file')} still cannot be converted at all.` : '');
      return;
    }
    /* The honest sentence, and the one users actually get. It names the output
       format, because "192 kbps MP3" for an afconvert run would be a false
       statement about the user's own file — the same class of error as a false
       statement about the device. */
    const af = state.encoder === 'afconvert';
    tag.textContent = `Converting with ${state.encoder} — playback unconfirmed`;
    body.textContent =
      (af
        ? 'Files your watch cannot play will be converted to 192 kbps AAC in M4A on this ' +
          'Mac, using macOS’s own afconvert — no ffmpeg needed. If you installed ffmpeg ' +
          'and expected it to be used, an app opened from Finder does not inherit the ' +
          'PATH a Homebrew ffmpeg lives on. '
        : 'Files your watch cannot play will be converted to 192 kbps MP3 on this Mac. ') +
      'Pelican has never confirmed that a watch’s music app plays this output — ' +
      'arriving, being indexed and playing are three different subsystems, and only ' +
      'arriving has been observed.' +
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
     there. On the attached FR165 `/Music` is durable — the owner's tracks
     survive across sessions and replugs — so a journal row the listing does
     not confirm most likely means the file has gone, and it says so rather
     than being counted as aboard. (No count here on purpose: the library
     grows, and a comment that pins one goes stale the next time he syncs.)

     The dedupe key is the *sanitised remote stem* on both sides: the shell
     journals the name it actually wrote to the device, so `u.name` and
     `stemOf(e.name)` are the same string by construction.

     Broken stubs come first — they are the only rows with anything to do. */
  function wallItems() {
    /* The journal, keyed the way the device names things. `history::forget`
       in Rust lowercases the same stem, which is what keeps a delete on this
       side and a forget on that side talking about the same file. */
    const byStem = new Map();
    for (const u of state.uploads) byStem.set(u.name.toLowerCase(), u);

    const seen = new Set();
    const broken = [];
    const onDevice = [];
    for (const e of state.entries) {
      if (e.isFolder) continue;
      const stem = stemOf(e.name);
      seen.add(stem);
      const u = byStem.get(stem);
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
        /* Two independent facts, and the wall's grouping depends on telling
           them apart: whether Pelican has any record of sending this file,
           and whether that record carried tags. A file with no journal row
           is never guessed at — see `wallGroups`. */
        journalled: !!u,
        title: u?.title || null,
        artist: u?.artist || null,
        album: u?.album || null,
        note: e.isBroken ? null : 'In /Music on the watch',
      });
    }
    const journal = [];
    for (const u of state.uploads) {
      if (seen.has(u.name.toLowerCase())) continue;
      journal.push({
        broken: false, name: u.name, raw: u.name, bytes: u.bytes, at: u.at,
        onDevice: false, journalled: true,
        title: u.title || null, artist: u.artist || null, album: u.album || null,
        note: `${sentWhen(u.at)} · the watch is not listing it now`,
      });
    }
    journal.sort((a, b) => (b.at || 0) - (a.at || 0));
    return [...broken, ...onDevice, ...journal];
  }

  /* Files / Album / Artist. `files` is the default and stays the default: it
     is the observed truth, the order `list_dir("Music")` came back in, and
     the app should open on the device's answer rather than on Pelican's
     memory of it. */
  let arrange = 'files';

  /* Exactly five provenance classes, in this order, each naming its source.
     The point of the ordering and the labels is that no row is ever placed
     under a claim the device did not make: a file the watch reports and
     Pelican has no record of goes into "Not from Pelican" with the filename
     the watch gave, never into an album group and never under "Unknown
     Artist" — which would read as a tag rather than as an absence. */
  function wallGroups(items) {
    const key = arrange === 'artist' ? 'artist' : 'album';
    const unreadable = items.filter((i) => i.broken);
    const rest = items.filter((i) => !i.broken);

    const tagged = [];
    const untagged = [];
    const foreign = [];
    const gone = [];
    for (const it of rest) {
      if (!it.onDevice) gone.push(it);
      else if (!it.journalled) foreign.push(it);
      else if (it[key]) tagged.push(it);
      else untagged.push(it);
    }

    /* Album key is album+artist, so two different "Greatest Hits" stay
       separate. Artist key is the artist alone. */
    const groups = new Map();
    for (const it of tagged) {
      const k = key === 'album'
        ? `${it.album.trim().toLowerCase()} \u0000 ${(it.artist || '').trim().toLowerCase()}`
        : it.artist.trim().toLowerCase();
      if (!groups.has(k)) {
        groups.set(k, {
          kind: 'tagged',
          label: key === 'album'
            ? (it.artist ? `${it.album} — ${it.artist}` : it.album)
            : it.artist,
          sort: key === 'album'
            ? `${(it.artist || '').toLowerCase()} ${it.album.toLowerCase()}`
            : it.artist.toLowerCase(),
          items: [],
        });
      }
      groups.get(k).items.push(it);
    }
    const out = [...groups.values()].sort((a, b) => a.sort.localeCompare(b.sort));
    for (const g of out) g.items.sort((a, b) => a.name.localeCompare(b.name));

    const blocks = [];
    if (unreadable.length) {
      blocks.push({
        label: `Unreadable (${unreadable.length})`,
        /* The first sentence is what Pelican observed. The second is labelled
           as inference, because it is one: the `is_broken` doc on
           `RemoteEntry` (`crates/pelican-core/src/mtp.rs` lines 20-23) and
           its DTO mirror (`crates/pelican-shell/src/dto.rs` lines 26-30) both
           hedge the cause with "almost always". The third is the record, and
           it is flat rather than hedged: `docs/garmin-mtp.md` §6 has every
           attempt returning `Protocol GeneralError`. */
        sub: 'The watch reports these handles but refuses their metadata. '
           + 'Most likely leftovers from rejected uploads. Your watch has '
           + 'refused every request to remove one; it clears them itself, on '
           + 'its own schedule.',
        items: unreadable,
      });
    }
    blocks.push(...out.map((g) => ({
      label: g.label,
      sub: `${plural(g.items.length, 'file')} · ${fmt(sumBytes(g.items))}`,
      items: g.items,
    })));
    if (untagged.length) {
      blocks.push({
        label: `${key === 'album' ? 'Album' : 'Artist'} unknown (${untagged.length})`,
        sub: 'Pelican sent these but did not record their tags.',
        items: untagged,
      });
    }
    if (foreign.length) {
      blocks.push({
        label: `Not from Pelican (${foreign.length})`,
        // Says what Pelican does, not what the watch cannot: this device
        // answers GetObjectPropValue for Artist and AlbumName even on files
        // we never sent (docs/garmin-mtp.md). Nothing calls 0x9801-0x9805 at
        // runtime, so "only asks for filenames" is exactly true and the
        // denial that used to sit here was not.
        sub: 'Pelican has no record of sending these, so it does not know '
           + 'their album or artist. It only asks the watch for filenames.',
        items: foreign,
      });
    }
    if (gone.length) {
      blocks.push({
        label: `Sent, not on the watch now (${gone.length})`,
        sub: 'Pelican’s own record. The watch is not listing these, so there '
           + 'is nothing on it to delete — only the record to forget.',
        items: gone,
      });
    }
    return blocks;
  }

  /* Device-reported sizes only. A journal row's byte count is what Pelican
     sent, not what the watch holds, and summing the two together would
     produce a total the device never gave. */
  const sumBytes = (items) =>
    items.reduce((t, i) => t + (i.onDevice && !i.broken ? i.bytes : 0), 0);

  function sentWhen(secs) {
    if (!secs) return 'Sent';
    const d = new Date(secs * 1000);
    const days = Math.floor((Date.now() - d.getTime()) / 86400000);
    if (days <= 0) return 'Sent today';
    if (days === 1) return 'Sent yesterday';
    if (days < 30) return `Sent ${days} days ago`;
    return 'Sent ' + d.toLocaleDateString();
  }

  /* What the user has ticked in the wall, and the two kinds are not the same
     action. A device path can be deleted; a journal name has nothing behind
     it to delete, so the only honest thing on offer is for Pelican to forget
     its own record. Keeping them in separate sets is what stops the second
     being dressed up as the first. */
  const wallPicks = new Set();    /* device paths  — deletable */
  const wallGhosts = new Set();   /* journal names — forgettable */

  function wallRow(tpl, it) {
    const frag = tpl.content.cloneNode(true);
    const li = frag.querySelector('li');
    $('.t', li).textContent = it.name;
    /* The row clips at the measure, so the full name has to stay reachable.
       CSS truncation does not touch the accessible name, so the text node is
       still complete for a screen reader; this is for the pointer. */
    li.title = it.raw;

    const pick = $('[data-pick]', li);
    if (it.broken) {
      li.dataset.broken = 'true';
      $('.s', li).textContent = 'The watch will not report this file';
      /* Not fmt(0). `GetObjectInfo` failed for this handle, so the watch
         never gave us a size — and "0 KB" would be one we invented. */
      $('.z', li).textContent = '—';
      pick.dataset.path = it.path;
      /* "Select … for removal", not "Remove": every stub-delete Pelican has
         issued has come back Protocol GeneralError, and the accessible name
         must not promise more than the visible confirmation does. */
      pick.setAttribute('aria-label',
        `Select the unreadable file ${it.name} for removal`);
      pick.checked = wallPicks.has(it.path);
    } else if (it.onDevice) {
      $('.s', li).textContent = it.note || '';
      $('.z', li).textContent = fmt(it.bytes);
      li.dataset.bytes = String(it.bytes);
      pick.dataset.path = it.path;
      /* "the file", for the reason the bar's button says "files": deleting the
         object is observed to work and to return the space; clearing the
         watch's music list is not something Pelican does. §8. */
      pick.setAttribute('aria-label', `Delete the file ${it.name} from your watch`);
      pick.checked = wallPicks.has(it.path);
    } else {
      $('.s', li).textContent = it.note || '';
      $('.z', li).textContent = fmt(it.bytes);
      pick.dataset.ghost = it.name;
      /* Never "delete": there is nothing on the watch to delete. */
      pick.setAttribute('aria-label', `Forget that Pelican sent ${it.name}`);
      pick.checked = wallGhosts.has(it.name);
    }
    li.classList.toggle('is-picked', pick.checked);
    return frag;
  }

  function wallGroupHead(block) {
    const frag = $('#tpl-wall-group').content.cloneNode(true);
    const li = frag.querySelector('li');
    $('.t', li).textContent = block.label;
    $('.s', li).textContent = block.sub;
    const pick = $('[data-pick-group]', li);
    pick.setAttribute('aria-label', `Select everything under ${block.label}`);
    /* Selects only this group's *deletable* members. A "Sent, not on the
       watch now" group has none on the device, so its members go into the
       forget set instead — the two are never conflated. */
    pick.dataset.keys = JSON.stringify(block.items.map(
      (i) => (i.onDevice ? { path: i.path } : { ghost: i.name })));
    const total = block.items.length;
    const on = block.items.filter(
      (i) => (i.onDevice ? wallPicks.has(i.path) : wallGhosts.has(i.name))).length;
    pick.checked = total > 0 && on === total;
    /* The standard signal for a partial selection, and no new visual. */
    pick.indeterminate = on > 0 && on < total;
    return frag;
  }

  function renderWall() {
    const items = wallItems();
    const tpl = $('#tpl-wall');

    /* Prune the selection to what still exists. A snapshot arrives on
       connect, after a delete and after a sync, and rows the device no longer
       lists must not stay in a set that a later Delete would act on. */
    const livePaths = new Set(items.filter((i) => i.path).map((i) => i.path));
    const liveGhosts = new Set(items.filter((i) => !i.onDevice).map((i) => i.name));
    for (const p of [...wallPicks]) if (!livePaths.has(p)) wallPicks.delete(p);
    for (const n of [...wallGhosts]) if (!liveGhosts.has(n)) wallGhosts.delete(n);

    wallList.textContent = '';
    if (arrange === 'files') {
      for (const it of items) wallList.appendChild(wallRow(tpl, it));
    } else {
      for (const block of wallGroups(items)) {
        wallList.appendChild(wallGroupHead(block));
        for (const it of block.items) wallList.appendChild(wallRow(tpl, it));
      }
    }
    updateWallbar();

    const brokenCount = items.filter((i) => i.broken).length;
    /* Count only what the device reported. A journal row is Pelican's own
       record of a send, not an observation of the watch, and counting the
       two together answers "what is on my watch" with a number the watch did
       not give. */
    const onWatch = items.filter((i) => !i.broken && i.onDevice).length;
    const stubLink = $('[data-pick-stubs]');
    wallCount.firstChild.textContent = brokenCount
      ? `${plural(onWatch, 'track')} aboard · ${brokenCount} unreadable · `
      : `${plural(onWatch, 'track')} aboard`;
    /* Both directions in one control, written from the live count. Scoped to
       every row the wall is listing, which in every arrangement is the same
       set — Files/Album/Artist rearrange one list, they do not filter it. */
    const allLink = $('[data-wall-all]');
    if (allLink) {
      const keys = items.map((i) => (i.path ? { path: i.path } : { ghost: i.name }));
      allLink.hidden = keys.length === 0;
      const on = keys.filter(
        (k) => (k.path ? wallPicks.has(k.path) : wallGhosts.has(k.ghost))).length;
      const all = keys.length > 0 && on === keys.length;
      allLink.textContent = all ? 'select none' : `select all ${keys.length}`;
      /* Label in Name, as on [data-lib-all]. Matching is case-insensitive, so
         the lowercase "select all 4" is contained in "Select all 4 files on
         your watch"; the lowercase style is the .linkbtn's, not a second
         label. Only the "none" branch had to change. */
      allLink.setAttribute('aria-label', all
        ? 'Select none — clear the selection'
        : `Select all ${plural(keys.length, 'file')} on your watch`);
      allLink.dataset.all = all ? 'yes' : 'no';
      allLink.dataset.keys = JSON.stringify(keys);
    }
    if (stubLink) {
      stubLink.hidden = brokenCount === 0;
      stubLink.textContent = brokenCount === 1
        ? 'select it' : `select all ${brokenCount}`;
      /* Singular was the Label-in-Name break here: "select it" against the
         name "Select the unreadable file" — the word "it" is not in the name,
         so the one thing a speech-input user could read off the screen was
         the one thing that did not work. Plural was already fine. */
      stubLink.setAttribute('aria-label', brokenCount === 1
        ? 'Select it — the unreadable file'
        : `Select all ${brokenCount} unreadable files`);
    }
    /* The count line ends with "· " only when there are stubs, so without
       this the select-all ran straight on from "6 tracks aboard" — and with
       stubs it ran straight on from "select it". One separator, shown
       whenever the link is. */
    const sep = $('[data-wall-sep]');
    if (sep) sep.hidden = !(allLink && !allLink.hidden);

    /* One scroll region does the work. A "Show all 20" link under a list that
       is already cut off by its own scroll promises a reveal that scrolling
       has already given. */
    if (wall) wall.classList.toggle('is-empty', state.connected && items.length === 0);

    if (state.total > 0) {
      setLevel((state.total - state.free) / state.total * 100);
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

  /* ── the wall's selection, and the one irreversible action ──────────── */

  /* Everything the bar and the confirm need to say, read off the DOM once so
     the two can never describe different selections. */
  function picked() {
    /* Group headers are excluded: their checkbox is a bulk control over the
       rows beneath, not a selection of its own. */
    const rows = $$('.wall__list li:not(.wallgroup)')
      .filter((li) => $('[data-pick]', li).checked);
    const files = rows.filter((li) => !li.dataset.broken && $('[data-pick]', li).dataset.path);
    const stubs = rows.filter((li) => li.dataset.broken);
    const ghosts = rows.filter((li) => $('[data-pick]', li).dataset.ghost);
    return {
      rows, files, stubs, ghosts,
      n: rows.length,
      /* Stubs contribute no bytes and must not contribute zeros: the watch
         never told us their size, and adding 0 would quietly assert it did. */
      bytes: files.reduce((t, li) => t + Number(li.dataset.bytes || 0), 0),
      names: (list) => list.map((li) => $('.t', li).textContent),
    };
  }

  function updateWallbar() {
    if (!wallbar) return;
    const p = picked();
    /* The bar and the confirm are alternatives, never both. */
    const confirming = confirmBox && !confirmBox.hidden;
    wallbar.hidden = p.n === 0 || confirming;
    if (p.n === 0) { closeConfirm(false); return; }

    const parts = [`${p.n} selected`];
    if (p.files.length) parts.push(`${fmt(p.bytes)} (${plural(p.files.length, 'file')})`);
    if (p.stubs.length) parts.push(`${p.stubs.length} unreadable, size unknown`);
    if (p.ghosts.length) parts.push(`${p.ghosts.length} not on the watch`);
    $('[data-wallbar-n]').textContent = parts.join(' · ');

    /* The button must not say a word Pelican cannot honour. Nothing on the
       device means nothing to delete; nothing but stubs means an outcome the
       firmware gets the last word on, and has so far always answered no
       (docs/garmin-mtp.md §6). Hence "Ask", never "Delete".

       "Delete these files", not "Delete from watch": §8 records a delete that
       emptied /Music and returned the space while the watch's music app went
       on listing every track. "From watch" is the reading a user would take
       as "off my watch", which is the one outcome Pelican has no evidence it
       can deliver. Files are what it removes, so files are what it says. */
    $('[data-wall-delete]').textContent =
      p.files.length ? 'Delete these files'
        : p.stubs.length ? 'Ask the watch to remove'
          : 'Forget these';
  }

  function nameList(names) {
    const shown = names.slice(0, 3).join(', ');
    return names.length > 3 ? `${shown} and ${names.length - 3} more` : shown;
  }

  /* Three facts, no euphemism, and a different sentence for each kind of
     selection — "delete 3 files" is not what a broken stub is, and a journal
     row with nothing behind it is not a deletion at all.

     A stub is the one selection where Pelican must promise an *attempt* and
     not a result, and must say which way the attempt has always gone.
     `docs/garmin-mtp.md` §6 and `docs/status.md` both record the same
     observation: every `DeleteObject` issued against a broken-stub handle on
     FR165 FW 2506 has returned `Protocol GeneralError`, and cleanup is
     watch-side and asynchronous. Pelican has never seen one succeed. So the
     copy must not read as a coin flip either: "it has refused before" invites
     a user to expect it might not this time, which is a claim nothing in the
     record supports. Lead with the record, offer the ask anyway because a
     finite sample on one firmware is not a law, and report whatever the watch
     actually answers. Word the request, report the answer, and never dress a
     documented refusal as a maybe. */
  function confirmCopy(p) {
    const body = $('[data-confirm-body]');
    body.textContent = '';
    /* The tag names what is at stake. A journal-only selection does not touch
       the watch at all, and a stub-only selection may not either — the watch
       gets the last word on that one, so the tag must not pre-empt it. */
    $('#confirm-tag').textContent =
      p.files.length ? 'This cannot be undone'
        : p.stubs.length ? 'Your watch has always refused this'
          : 'This clears Pelican’s record';
    const strong = (t) => Object.assign(document.createElement('strong'), { textContent: t });
    const text = (t) => document.createTextNode(t);

    /* What Pelican observed about a stub is exactly one thing: the handle is
       listed and `GetObjectInfo` fails on it. Everything else — that it is a
       rejected upload, that it holds no playable audio — is inference, which
       is why the engine hedges the cause rather than asserting it: see the
       `is_broken` doc on `RemoteEntry` (`crates/pelican-core/src/mtp.rs`
       lines 20-23) and its DTO mirror (`crates/pelican-shell/src/dto.rs`
       lines 26-30), both of which say "almost always". */
    const stubWhy = (n) => (n === 1
      ? 'The watch lists this handle but will not report its metadata; it is '
        + 'most likely a leftover from an upload the watch rejected. '
      : 'The watch lists these handles but will not report their metadata; '
        + 'they are most likely leftovers from uploads the watch rejected. ');
    /* The record, not a probability. Every time Pelican has asked, the answer
       was Protocol GeneralError. The ask is still offered — one firmware is
       not every firmware — but it is offered as an ask that has never yet
       worked, and the removal that does happen is the watch's own. */
    const stubAsk = (n) => 'Your watch has refused to remove '
      + (n === 1 ? 'a file like this' : 'files like these')
      + ' every time Pelican has asked. Pelican will ask again and tell you '
      + 'exactly what the watch says, but expect a refusal: on this firmware '
      + (n === 1 ? 'the only thing that clears it is the watch itself, '
                 : 'the only thing that clears them is the watch itself, ')
      + 'on its own schedule.';

    const onDevice = [...p.files, ...p.stubs];
    if (onDevice.length === 0) {
      $('[data-confirm-go]').textContent = 'Forget';
      body.append(
        text('Pelican will forget that it sent '),
        strong(nameList(p.names(p.ghosts))),
        text('. Nothing on your watch changes — the watch is not listing '
          + (p.ghosts.length === 1 ? 'this file' : 'these files')
          + ', so there is nothing there to remove.'));
      return;
    }

    if (p.files.length === 0) {
      /* Not "Delete": the button must not name an outcome the watch has never
         once agreed to. "anyway" is doing real work — it says the expected
         answer is no. */
      $('[data-confirm-go]').textContent = 'Ask anyway';
      body.append(
        strong(`${plural(p.stubs.length, 'unreadable file')}`),
        text('. ' + stubWhy(p.stubs.length) + stubAsk(p.stubs.length)));
    } else {
      $('[data-confirm-go]').textContent = 'Delete';
      /* Two different objects, and the confirmation is the last place they can
         be told apart before the user commits. The *file* goes: §8 measured
         /Music down to one entry and 78.5 MB of free space returned, so
         "removed" and "frees the space" are both observed. The watch's own
         music *list* is a separate thing Pelican has never touched, and after
         that same delete it still held all 22 entries. Saying only the first
         half would let the user read a promise Pelican has no evidence it can
         keep. Saying the second half as settled would be its own overclaim —
         one delete, one firmware, mechanism unknown — hence "may not" and
         "the one time". "Pelican has no way to reach that list" is the one
         flat assertion here, and it is a fact about this codebase rather than
         about the device. */
      /* Lead with the two figures a person is actually deciding about, both
         observed. `nameList` caps at three names plus "and N more", so with
         select-all this read "A, B, C and 19 more will be removed" — the
         count 22 was never stated, only reconstructible by adding three to
         nineteen — and the weight was never stated at all, though `picked()`
         had already summed it from device-reported sizes and the wallbar one
         line up was displaying it. The stub branch above got this right and
         the readable branch was the outlier.

         "Your watch reports these as X" and not "this will free X": the
         prediction belongs to the report after the fact, where the number is
         the device's own answer rather than a sum of what it said earlier.
         Stubs contribute no bytes here — `picked()` refuses to add zeros for
         handles the watch never sized, and that discipline has to survive
         into the sentence. */
      body.append(
        strong(plural(p.files.length, 'file')),
        text(' — '),
        strong(fmt(p.bytes)),
        text(', including '),
        strong(nameList(p.names(p.files))),
        text('. '),
        text((p.files.length === 1 ? 'It' : 'They')
          + ' will be removed from your watch’s storage. Pelican cannot undo '
          + 'this, and the watch has no trash. Your copies on this Mac are '
          + 'not touched. Free space now: '),
        strong(fmt(state.free)),
        text('.'),
        text(' Removing '
          + (p.files.length === 1 ? 'it' : 'them')
          + ' frees that space — but it may not clear '
          + (p.files.length === 1 ? 'the track' : 'the tracks')
          + ' from the watch’s music app. The one time this was tried, the '
          + 'app still listed everything, and Pelican has no way to reach '
          + 'that list. Pelican will tell you what changed.'));
      /* A mixed selection cannot inherit the readable files' certainty: the
         same batch, two different confidences. */
      if (p.stubs.length) {
        body.append(text(' Pelican will also ask the watch to remove '),
          strong(`${plural(p.stubs.length, 'unreadable file')}`),
          text(' — but your watch has refused that every time it has been '
            + 'asked, so expect it to stay. Pelican will report what the '
            + 'watch says.'));
      }
    }
    if (p.ghosts.length) {
      body.append(text(' Pelican will also forget that it sent '),
        strong(nameList(p.names(p.ghosts))),
        text(' — nothing on your watch changes for '
          + (p.ghosts.length === 1 ? 'that one' : 'those')
          + ', because the watch is not listing '
          + (p.ghosts.length === 1 ? 'it' : 'them') + '.'));
    }
  }

  function openConfirm() {
    const p = picked();
    if (!p.n) return;
    confirmCopy(p);
    confirmBox.hidden = false;
    wallbar.hidden = true;
    /* Focus lands on the safe option. */
    $('[data-confirm-keep]').focus();
  }

  /* ── what the delete actually did ───────────────────────────────────── */

  const dreport = $('[data-dreport]');

  function hideDeleteReport() {
    if (dreport) dreport.hidden = true;
  }

  /* Two measured quantities and one caveat. Every figure here came off the
     device: `ok`/`failed` are the engine's own tallies of DeleteObject
     results, and the two free readings are consecutive snapshots.

     It deliberately does not say "freed 78.5 MB" as a prediction, and it
     deliberately does not pick between docs/garmin-mtp.md §8's two open
     models. The one flat assertion is about this codebase — Pelican has no
     operation that touches the watch's music library — and the observation
     is reported as the single observation it is. */
  function renderDeleteReport(pd, freeAfter) {
    if (!dreport) return;
    const { ok, failed, freeBefore } = pd;
    const body = $('[data-dreport-body]');
    const text = (s) => document.createTextNode(s);
    const strong = (s) => { const e = document.createElement('strong'); e.textContent = s; return e; };

    $('[data-dreport-tag]').textContent = ok
      ? `${plural(ok, 'file')} removed`
      : 'Nothing was removed';

    body.textContent = '';
    if (ok) {
      body.append(
        strong(plural(ok, 'file')),
        text(' removed from your watch’s storage. Free space went from '),
        strong(fmt(freeBefore)), text(' to '), strong(fmt(freeAfter)), text('.'));
    }
    if (failed) {
      if (ok) body.append(text(' '));
      body.append(
        strong(plural(failed, 'file')),
        text(failed === 1
          ? ' could not be removed. The watch’s reason:'
          : ' could not be removed. The watch’s reason for each:'));
      const why = document.createElement('ul');
      why.className = 'report__why';
      for (const f of (pd.failures || [])) {
        const li = document.createElement('li');
        li.append(strong(f.name), text(` — ${f.error}`));
        why.append(li);
      }
      if (why.childElementCount) body.append(why);
    }
    if (ok) {
      body.append(text(
        ' Your watch’s music app may still list '
        + (ok === 1 ? 'it' : 'them')
        + '. Pelican removed the file' + (ok === 1 ? '' : 's')
        + '; it has no operation that touches the watch’s own library, and '
        + 'the one time this was measured the app went on listing every '
        + 'track.'));
    }

    dreport.hidden = false;
    /* Focus follows the outcome. `closeConfirm` restores focus carefully and
       this path did not: it hid the confirm while focus was on the Delete
       button inside it, so focus fell to <body> and a keyboard user who had
       just done the one irreversible thing in the app was returned to the top
       of the document with no announcement. Now they land on the report. */
    dreport.focus();
  }

  $('[data-dreport-dismiss]')?.addEventListener('click', () => {
    hideDeleteReport();
    /* Back into the panel that was acted on, never to <body>. */
    $('[data-wall-h]')?.focus();
  });

  function closeConfirm(restoreFocus) {
    if (!confirmBox || confirmBox.hidden) return;
    confirmBox.hidden = true;
    updateWallbar();
    if (restoreFocus && !wallbar.hidden) $('[data-wall-delete]').focus();
  }

  function runDelete() {
    const p = picked();
    const paths = p.rows.map((li) => $('[data-pick]', li).dataset.path).filter(Boolean);
    const names = p.rows.map((li) => $('[data-pick]', li).dataset.ghost).filter(Boolean);
    /* The free figure as the device last reported it, captured before the
       request goes out. The engine emits `Deleted` and only then re-snapshots
       (device.rs), so the "after" number arrives one event later — these two
       readings are what the report subtracts, and both are the watch's own
       answer rather than an estimate from the sizes in the wall. */
    state.pendingDelete = paths.length
      ? { freeBefore: state.free, requested: paths.length, ok: 0, failed: 0, settled: false }
      : null;
    hideDeleteReport();
    confirmBox.hidden = true;
    /* Both post to the one device thread and are handled in order. The
       snapshot each produces is what closes the loop on screen; nothing here
       predicts the outcome. */
    if (paths.length) {
      invoke('delete_remote', { paths })
        /* The request never reached the engine, so there is no measurement to
           report and the pending reading has to go — a stale "before" figure
           would otherwise be subtracted from an unrelated later snapshot. */
        .catch((e) => { state.pendingDelete = null; showError('Could not delete that', String(e)); });
    }
    if (names.length) {
      invoke('forget_uploads', { names })
        .catch((e) => showError('Could not update Pelican’s record', String(e)));
    }
  }

  /* Anchor for the wall's shift-range, as an index into the rendered rows.
     Group headers are not rows and never become the anchor. */
  let lastWallIndex = null;

  const wallRows = () => $$('.wall__list li:not(.wallgroup)');

  const setWallPick = (pick, on) => {
    const key = pick.dataset.path || pick.dataset.ghost;
    if (!key) return;
    const set = pick.dataset.path ? wallPicks : wallGhosts;
    if (on) set.add(key); else set.delete(key);
  };

  wallList.addEventListener('click', (e) => {
    const pick = e.target.closest('[data-pick]:not([data-pick-group])');
    if (!pick || !e.shiftKey || lastWallIndex === null) return;
    const rows = wallRows();
    const i = rows.indexOf(pick.closest('li'));
    if (i < 0) return;
    const [a, b] = i < lastWallIndex ? [i, lastWallIndex] : [lastWallIndex, i];
    for (let k = a; k <= b; k++) {
      const box = $('[data-pick]', rows[k]);
      if (box) setWallPick(box, pick.checked);
    }
    /* Repaint once from the sets. The `change` that follows this click will
       re-apply the clicked box's own state, which is already what it is. */
    renderWall();
    closeConfirm(false);
    hideDeleteReport();
  });

  wallList.addEventListener('change', (e) => {
    const group = e.target.closest('[data-pick-group]');
    if (group) {
      for (const k of JSON.parse(group.dataset.keys)) {
        if (k.path) { if (group.checked) wallPicks.add(k.path); else wallPicks.delete(k.path); }
        else { if (group.checked) wallGhosts.add(k.ghost); else wallGhosts.delete(k.ghost); }
      }
      /* Repaint from the sets rather than walking siblings: the group's rows
         are the ones between this header and the next, and depending on that
         adjacency would break the first time a group renders empty. */
      lastWallIndex = null;
      renderWall();
      closeConfirm(false);
      hideDeleteReport();
      return;
    }
    const pick = e.target.closest('[data-pick]');
    if (!pick) return;
    const li = pick.closest('li');
    li.classList.toggle('is-picked', pick.checked);
    setWallPick(pick, pick.checked);
    lastWallIndex = wallRows().indexOf(li);
    /* A change to the selection invalidates the sentence the confirm is
       showing, so it closes rather than confirming a stale list. The
       post-delete report goes for the same reason: it describes a batch that
       is no longer the one selected. */
    closeConfirm(false);
    hideDeleteReport();
    updateWallbar();
    /* A group header's tri-state is a function of its members, so it has to
       be recomputed whenever one of them changes. */
    if (arrange !== 'files') refreshGroupChecks();
  });

  function refreshGroupChecks() {
    for (const g of $$('[data-pick-group]')) {
      const keys = JSON.parse(g.dataset.keys);
      const on = keys.filter(
        (k) => (k.path ? wallPicks.has(k.path) : wallGhosts.has(k.ghost))).length;
      g.checked = keys.length > 0 && on === keys.length;
      g.indeterminate = on > 0 && on < keys.length;
    }
  }

  $$('[data-arrange]').forEach((chip) => {
    chip.addEventListener('click', () => {
      arrange = chip.dataset.arrange;
      $$('[data-arrange]').forEach((c) =>
        c.setAttribute('aria-pressed', String(c === chip)));
      /* The note explains a claim that only exists in the grouped modes. */
      $('[data-wall-note]').hidden = arrange === 'files';
      closeConfirm(false);
      renderWall();
    });
  });

  $('[data-wall-clear]')?.addEventListener('click', () => {
    wallPicks.clear(); wallGhosts.clear();
    lastWallIndex = null;
    /* Repaint from the cleared sets, which is also what refreshes the
       select-all link's label and the arrangement's group headers. */
    renderWall();
    closeConfirm(false);
    hideDeleteReport();
  });

  $('[data-wall-delete]')?.addEventListener('click', openConfirm);
  $('[data-confirm-keep]')?.addEventListener('click', () => closeConfirm(true));
  $('[data-confirm-go]')?.addEventListener('click', runDelete);

  $('[data-wall-all]')?.addEventListener('click', () => {
    const link = $('[data-wall-all]');
    const on = link.dataset.all !== 'yes';
    for (const k of JSON.parse(link.dataset.keys || '[]')) {
      if (k.path) { if (on) wallPicks.add(k.path); else wallPicks.delete(k.path); }
      else if (on) wallGhosts.add(k.ghost); else wallGhosts.delete(k.ghost);
    }
    /* Mutate the sets, repaint once — the same discipline the group header
       already uses. */
    lastWallIndex = null;
    renderWall();
    closeConfirm(false);
    hideDeleteReport();
  });

  $('[data-pick-stubs]')?.addEventListener('click', () => {
    /* Was one synthetic `change` per checkbox, each re-entering the wall's
       change handler, which calls `picked()` — a fresh query over the whole
       list — and, in the grouped arrangements, `refreshGroupChecks()`, which
       JSON-parses every header's keys. N full-list queries plus N×G parses
       for one click. Now: mutate the set, repaint once, like everything
       else in this panel. */
    for (const li of $$('.wall__list li[data-broken]')) {
      const pick = $('[data-pick]', li);
      if (pick?.dataset.path) wallPicks.add(pick.dataset.path);
      else if (pick?.dataset.ghost) wallGhosts.add(pick.dataset.ghost);
    }
    lastWallIndex = null;
    renderWall();
    closeConfirm(false);
    hideDeleteReport();
    $('[data-wall-delete]')?.focus();
  });

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

  /* The playable rows, in the order they are shown, and where we are in them.
     Previous and Next were enabled controls with no click handler anywhere —
     `.player__transport .iconbtn` appeared in the markup and nowhere in this
     file — and `ended` only re-painted the chrome, so playback stopped dead
     at the end of every track. The same two functions drive the footer
     buttons, the media keys and Control Center, so those three can never
     disagree about what "next" means. */
  let queue = [];
  let qi = -1;

  /* Ogg rows are excluded because they are disabled for playback: the media
     element reports readyState 4, a correct duration and a moving clock, and
     emits digital silence. A queue that walks into one would look hung. */
  const buildQueue = () => {
    /* Reads `aria-disabled`, not `.disabled`. The Ogg rows moved to the ARIA
       form so their refusal stays reachable by keyboard, and a queue that
       still tested the property would have swept them back in and played
       digital silence with a moving clock — the exact failure `OGG_SILENT`
       exists to prevent. */
    queue = $$('.tracks tbody tr').filter(
      (r) => $('.tracks__play .iconbtn', r)?.getAttribute('aria-disabled') !== 'true');
    qi = current ? queue.indexOf(current) : -1;
  };

  function next() {
    if (qi < 0 || qi >= queue.length - 1) return;
    load(queue[qi + 1]);
  }

  function prev() {
    if (qi < 0) return;
    /* The conventional behaviour, and the one every player on this machine
       has: past three seconds, Previous restarts the track. */
    if (audio.currentTime > 3) { audio.currentTime = 0; return; }
    if (qi > 0) load(queue[qi - 1]);
  }

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
        /* The Ogg rows are `aria-disabled`, not `disabled`, so that the
           reason stays reachable by keyboard — which means the refusal has
           to be enforced here instead of by the browser. */
        if (btn.getAttribute('aria-disabled') === 'true') return;
        const row = btn.closest('tr');
        if (current === row && !audio.paused) { audio.pause(); return; }
        if (current === row && audio.src) { audio.play().catch(reportPlayFailure); return; }
        load(row);
      });
    });
  }

  async function load(row) {
    /* The remote path — a media key, or the Control Center button — has to
       reach `play()` without an await in the way. A user-activation token
       from a MediaSession action handler is not something to bet a skip
       button on surviving an async IPC round trip and a fresh `src`.
       `set_now_playing` is pure string work after a scope check (its own doc
       comment says so), so its answer is stable for the life of the grant and
       can be memoised. The invoke still runs once per row, so the scope check
       is never skipped for a file. */
    if (row.dataset.url) {
      current = row;
      qi = queue.indexOf(row);
      audio.src = row.dataset.url;
      audio.play().catch((e) => reportPlayFailure(e, row));
      syncPlayerChrome();
      return;
    }
    try {
      /* The URL carries the true on-disk path. WKWebView picks its decoder
         from the extension, so an opaque id would break FLAC and WAV even
         though the bytes are identical. */
      const url = await invoke('set_now_playing', { path: row.dataset.path });
      row.dataset.url = url;
      current = row;
      qi = queue.indexOf(row);
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

  /* ── Control Center and the media keys ──────────────────────────────── */

  /* Verified present on this machine (macOS 26.6.2 build 25G83, wry 0.55.1 /
     Tauri 2.11.5) with compiled WKWebView harnesses: `navigator.mediaSession`
     and `MediaMetadata` exist, `setActionHandler` is accepted for play, pause,
     stop, seekbackward, seekforward, previoustrack, nexttrack and seekto, and
     `setPositionState` is accepted. Still guarded, because a capability that
     is present today is not a capability to assume. */
  const MS = ('mediaSession' in navigator) ? navigator.mediaSession : null;

  /* Cover art, one read per album, at play time — never during a scan.
     Keyed on album+artist so a twelve-track album costs one read. */
  const artCache = new Map();
  const artPending = new Map();

  const albumKey = (row) =>
    `${(row.dataset.album || '').toLowerCase()}\u0000${(row.dataset.artist || '').toLowerCase()}`;

  function wantArt(row) {
    const key = albumKey(row);
    if (artCache.has(key)) { paintArt(artCache.get(key)); return; }
    if (artPending.has(row.dataset.path)) return;
    artPending.set(row.dataset.path, key);
    invoke('cover_art', { path: row.dataset.path })
      /* A refusal is not worth an error box: the frame simply stays a frame,
         which is what it already is. */
      .catch(() => artPending.delete(row.dataset.path));
  }

  /* One string, three consumers: the hero frame, the player frame and the
     MediaMetadata artwork. Same directive, same allowance, one read. */
  function paintArt(dataUrl) {
    for (const el of $$('.cover--lg, .cover--sm')) {
      el.style.backgroundImage = dataUrl ? `url("${dataUrl}")` : '';
      el.style.backgroundSize = dataUrl ? 'cover' : '';
    }
    publishMediaSession();
  }

  function publishMediaSession() {
    if (!MS) return;
    if (!current) { MS.metadata = null; return; }
    const art = artCache.get(albumKey(current));
    MS.metadata = new MediaMetadata({
      title: current.dataset.title || $('.t', current).textContent,
      artist: current.dataset.artist || '',
      album: current.dataset.album || '',
      /* Omitted entirely when there is none. macOS then shows the app's own
         icon, which is true, instead of a generic note glyph, which would
         imply Pelican looked and found something. */
      artwork: art ? [{ src: art, sizes: '512x512', type: mimeOf(art) }] : [],
    });
    /* Re-registered on every change, so skip is not advertised at the ends of
       the queue: macOS greys the Control Center buttons from the published
       command set, and a permanently-registered handler shows two always-lit
       buttons, one of which silently does nothing. */
    MS.setActionHandler('nexttrack', qi >= 0 && qi < queue.length - 1 ? next : null);
    MS.setActionHandler('previoustrack', qi > 0 ? prev : null);
  }

  const mimeOf = (dataUrl) => {
    const m = /^data:([^;]+);/.exec(dataUrl);
    return m ? m[1] : 'image/jpeg';
  };

  if (MS) {
    MS.setActionHandler('play', () => {
      if (audio.src) audio.play().catch(reportPlayFailure);
    });
    MS.setActionHandler('pause', () => audio.pause());
    MS.setActionHandler('seekto', (d) => {
      if (typeof d.seekTime === 'number') audio.currentTime = d.seekTime;
    });
    /* `playbackState` is writable but WebKit re-derives it from the element —
       it read back `paused` after a tone ended despite being set to
       `playing`. So it is not managed by hand; the element's own play/pause
       events are the source of truth. */
  }

  /* Guarded on a finite duration, so Control Center's scrubber is never
     handed a NaN or an Infinity — a streaming-shaped duration would make the
     position bar a lie rather than an absence. */
  function publishPosition() {
    if (!MS || !MS.setPositionState) return;
    if (!Number.isFinite(audio.duration) || audio.duration <= 0) return;
    try {
      MS.setPositionState({
        duration: audio.duration,
        position: Math.min(audio.currentTime, audio.duration),
        playbackRate: audio.playbackRate || 1,
      });
    } catch { /* a rate or position the platform refuses is not worth a box */ }
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
      wantArt(current);
    }
    publishMediaSession();
  }

  audio.addEventListener('play', syncPlayerChrome);
  audio.addEventListener('pause', syncPlayerChrome);
  /* Playback used to stop dead here. */
  audio.addEventListener('ended', () => { syncPlayerChrome(); next(); });
  audio.addEventListener('error', () => { if (audio.src) reportPlayFailure(new Error('load failed')); });

  audio.addEventListener('loadedmetadata', () => {
    const d = Number.isFinite(audio.duration) ? audio.duration : 0;
    seek.max = String(d || 0);
    $('[data-time-total]').textContent = mmss(d);
    publishPosition();
  });

  audio.addEventListener('timeupdate', () => {
    if (seeking) return;
    seek.value = String(audio.currentTime);
    $('[data-time-now]').textContent = mmss(audio.currentTime);
    paintSeek();
    publishPosition();
  });

  $$('.player__transport .iconbtn').forEach((btn) => {
    const back = btn.getAttribute('aria-label') === 'Previous track';
    btn.addEventListener('click', back ? prev : next);
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
     transfer. Keep the value and the bar in step from one place.

     `aria-valuenow` is the same claim as the visible bar, aimed at the user
     least able to check it: a control labelled "Transfer progress" reading
     100 says the transfer completed. It must never be set to full by anything
     other than a full transfer. */
  function setProgress(pct, text) {
    if (!meter) return;
    meter.setAttribute('aria-valuenow', String(Math.round(pct)));
    /* A percentage is not the fact the user wants read out. `aria-valuetext`
       overrides it with the same sentence the sighted user gets. */
    if (text) meter.setAttribute('aria-valuetext', text);
    else meter.removeAttribute('aria-valuetext');
    const fill = meter.querySelector('i');
    if (fill) fill.style.transform = `scaleX(${(pct / 100).toFixed(3)})`;
  }
  setProgress(0);

  const notices = () => $('[data-notices]');

  /* `action` is `{label, run}` for the one notice the user can act on. Every
     other notice leaves the second grid column collapsed. */
  /* `tone` is 'info' for an outcome the user should read but need not act on.
     It is not decoration: Signal Red is spent, per DESIGN.md, on four things
     that are all bad news, and a successful rename is none of them. */
  function addNotice(tag, body, action, tone) {
    const frag = $('#tpl-notice').content.cloneNode(true);
    if (tone === 'info') $('.notice', frag).classList.add('notice--info');
    $('.notice__tag', frag).textContent = tag;
    /* Verbatim. The engine's message is the only place that says whether a
       failed write left a stub behind, and therefore whether retrying is
       safe. Summarising it would throw that away. */
    $('.notice__body', frag).textContent = body;
    if (action) {
      const acts = $('.notice__acts', frag);
      const btn = $('[data-notice-act]', frag);
      acts.hidden = false;
      btn.textContent = action.label;
      btn.addEventListener('click', () => {
        /* One post per offer. A second click would queue a second run
           against a plan the first one is already changing. */
        btn.disabled = true;
        action.run();
      });
    }
    notices().appendChild(frag);
  }

  const SKIP_LABEL = {
    notAudio: 'Not audio', notPlayable: 'Cannot play as-is',
    missingTags: 'Needs a title',
    /* The one skip the user has to act on: nothing was sent, and nothing
       will be until they remove the file on the watch or send this one under
       a different name. */
    nameTaken: 'Name already on the watch',
    other: 'Skipped',
  };
  const FAIL_LABEL = {
    deviceBusy: 'Watch is held by something else', openSession: 'Could not open the watch',
    noEncoder: 'No converter for this format', sizeMismatch: 'Wrote the wrong size',
    other: 'Failed',
  };

  function startRun(total, bytes) {
    state.run = {
      phase: 'running', total, bytes, jobBytes: 0, startedAt: Date.now(),
      ok: 0, skipped: 0, failed: 0, stopped: false,
      delivered: 0, planned: bytes, landed: [], dismissed: false,
      /* Local paths a name collision refused. Collected as they arrive so
         the finished report can offer to re-send exactly those. */
      nameTaken: [],
    };
    /* A finished report is superseded by the next run, and by nothing else —
       never by a timer. See `finishRun`. */
    notices().textContent = '';
    /* Live again for the new run. `finishRun` turns it off, because a
       finished report announced as a live region tells a screen-reader user a
       transfer is in progress. */
    transfer.setAttribute('aria-live', 'polite');
    const stop = $('[data-stop]');
    stop.hidden = false;
    stop.disabled = false;
    stop.textContent = 'Stop after this track';
    $('[data-dismiss]').hidden = true;
    $('[data-transfer-total]').textContent = String(total);
    $('[data-transfer-done]').textContent = '0';
    $('[data-transfer-breakdown]').textContent = '';
    $('[data-transfer-eta]').textContent = '';
    $('[data-now-label]').textContent = 'Preparing…';
    $('[data-now-bytes]').textContent = '';
    meter.dataset.outcome = 'running';
    setProgress(0);
    applyState();
  }

  function paintRun(ev) {
    const run = state.run;
    if (!run) return;
    run.ok = ev.ok; run.skipped = ev.skipped; run.failed = ev.failed;

    $('[data-transfer-done]').textContent = String(ev.completed);
    $('[data-transfer-total]').textContent = String(ev.total);
    $('[data-transfer-breakdown]').textContent = breakdown(ev);
    /* Once there is a byte total, the byte-weighted figure is authoritative
       and monotone. Falling back to `completed / total` on the events that
       carry no `jobBytes` — fileStarted, fileStaging, fileDone — snapped the
       bar backwards every time a file finished. */
    if (run.bytes > 0) {
      if (typeof ev.jobBytes === 'number') run.jobBytes = Math.max(run.jobBytes, ev.jobBytes);
      setProgress(Math.min(100, run.jobBytes / run.bytes * 100));
    } else if (ev.total > 0) {
      setProgress(ev.completed / ev.total * 100);
    }
    /* Recomputed on every event, so a long staging phase cannot leave a
       stale estimate on screen. */
    $('[data-transfer-eta]').textContent = eta();
  }

  /* "3 of 5" is where the run is, not how it is going: the numerator counts
     skips and failures too — device.rs says so at length and the reasoning is
     right. What it cannot do is stand alone, because read bare it says "3
     sent". So the breakdown rides beside it whenever the two differ, and the
     owner's sighting reads "1 of 1 · 1 failed" rather than "1 of 1". */
  function breakdown(ev) {
    if (!ev.skipped && !ev.failed) return '';
    const parts = [];
    if (ev.ok) parts.push(`${ev.ok} sent`);
    if (ev.skipped) parts.push(`${ev.skipped} skipped`);
    if (ev.failed) parts.push(`${ev.failed} failed`);
    return ' · ' + parts.join(', ');
  }

  function eta() {
    const run = state.run;
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

  /* The run ends. Everything present-tense about the card has to stop being
     present tense in the same breath: the heading, the Stop button that now
     controls nothing, the live region, and the bar. */
  function finishRun(ev) {
    const run = state.run || { total: 0, bytes: 0 };
    Object.assign(run, {
      phase: 'finished',
      ok: ev.ok, skipped: ev.skipped, failed: ev.failed, stopped: ev.stopped,
      delivered: ev.deliveredBytes, planned: ev.plannedBytes,
      landed: [], dismissed: false,
    });
    state.run = run;

    /* A full bar means everything landed and nothing else. `setProgress(100)`
       was unconditional here, so a run that sent nothing still finished full
       — the same lie as the counter, and `aria-valuenow="100"` aimed it at
       the person least able to check it. */
    const pct = ev.plannedBytes > 0
      ? (ev.deliveredBytes / ev.plannedBytes) * 100
      : (run.total > 0 ? (ev.ok / run.total) * 100 : 0);
    /* Kept on the run so a later reconciliation can rewrite the meter's
       `aria-valuetext` without inventing a second percentage. */
    run.pct = Math.max(0, Math.min(100, pct));
    setProgress(run.pct, outcomeHeading(run));
    meter.dataset.outcome =
      ev.stopped ? 'stopped'
      : ev.failed ? 'failed'
      : ev.skipped ? 'partial'
      : 'clean';

    $('[data-transfer-breakdown]').textContent = breakdown(ev);
    $('[data-now-label]').textContent = outcomeHeading(run) + '.';
    $('[data-now-bytes]').textContent = '';
    $('[data-transfer-eta]').textContent = '';

    /* Hidden, not re-enabled. Re-enabling it put a live-looking control on a
       finished run that it no longer had anything to stop. */
    const stop = $('[data-stop]');
    stop.hidden = true;
    $('[data-dismiss]').hidden = false;

    /* The one skip with something to do about it. Each refused track already
       has its own verbatim notice above; this adds the affordance and the
       one fact those notices cannot carry — that renaming leaves what is
       already on the watch alone. */
    const taken = run.nameTaken || [];
    if (taken.length) {
      const n = taken.length;
      addNotice(
        'Name already on the watch',
        `${n} ${n === 1 ? 'track was' : 'tracks were'} not sent, because the watch already ` +
        `has a file under that name. Sending under a new name leaves what is already ` +
        `on your watch untouched.`,
        { label: 'Send under a new name', run: () => sendUnderNewNames(taken) },
      );
    }

    /* Announce the terminal sentence, then stop claiming to be live. The
       order matters: turning the region off first would swallow the one
       announcement the user needs. */
    setTimeout(() => {
      if (state.run && state.run.phase === 'finished') {
        transfer.setAttribute('aria-live', 'off');
      }
    }, 0);

    /* A clean run fades out through the existing @starting-style transition.
       A run with anything to report stays until it is dismissed or the next
       run supersedes it — a failure record must not vanish on a timer. */
    if (ev.failed === 0 && ev.skipped === 0 && !ev.stopped) {
      setTimeout(() => {
        if (state.run === run && run.phase === 'finished') dismissRun();
      }, 1400);
    }
    applyState();
  }

  function dismissRun() {
    if (!state.run) return;
    state.run.dismissed = true;
    applyState();
  }

  $('[data-dismiss]')?.addEventListener('click', dismissRun);
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    /* The confirm outranks everything: Escape's first job is always to back
       out of the irreversible thing. Then the reports, newest concern first.
       Only then the selection — clearing 40 ticks is recoverable, backing out
       of a delete is the thing you cannot get wrong. */
    if (confirmBox && !confirmBox.hidden) { closeConfirm(true); return; }
    if (dreport && !dreport.hidden) {
      hideDeleteReport();
      $('[data-wall-h]')?.focus();
      return;
    }
    if (state.run && state.run.phase === 'finished' && !state.run.dismissed) {
      dismissRun();
      return;
    }
    /* A user who has just ticked 40 rows and changed their mind had no
       gesture but 40 more clicks. */
    if (wallPicks.size || wallGhosts.size) {
      wallPicks.clear(); wallGhosts.clear();
      lastWallIndex = null;
      renderWall();
      return;
    }
    if (libPicks.size) {
      libPicks.clear();
      lastLibIndex = null;
      for (const box of $$('.tracks__pick input')) box.checked = false;
      updateSelection();
    }
  });

  /* ── the send gesture ───────────────────────────────────────────────── */

  /* Rows sink toward the water. The water itself is NOT moved here: it rises
     when the device reports its new free space, which is the only moment the
     claim is true. Honours prefers-reduced-motion by skipping the animation. */
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');

  function send() {
    const tracks = checked();
    if (!tracks.length) return;
    /* From the set, not from the DOM — so a batch assembled across two albums
       goes across whole, rather than as whichever page happens to be open. */
    const paths = tracks.map((t) => t.path);
    /* Only the selected rows that are on screen can be animated. */
    const rows = checkedRows();

    /* skip_tag_check is true on purpose, and the copy above depends on it:
       an untagged file must transfer and merely stay invisible. With it off
       the engine would silently skip those files and the warning would be a
       lie. */
    invoke('start_sync', { paths, skipTagCheck: true })
      .catch((e) => showError('Could not start the transfer', String(e)));

    const settle = () => {
      libPicks.clear();
      lastLibIndex = null;
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

  /* Re-send exactly the tracks a collision refused, under names free on the
     watch. `onConflict: 'rename'` is the user's answer to a collision they
     were shown — never a default, and never applied to anything but the
     paths they were shown it for. */
  function sendUnderNewNames(items) {
    const paths = items.map((i) => i.path);
    if (!paths.length) return;
    invoke('start_sync', { paths, skipTagCheck: true, onConflict: 'rename' })
      .catch((e) => showError('Could not start the transfer', String(e)));
  }

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
        /* A successful open is direct evidence against "The watch would not
           open", and leaving that box up over a watch that demonstrably just
           opened is the same class of untruth as the wrong counter. It used
           to be cleared only by an explicit Reconnect or by picking a folder,
           so it survived every later success. */
        clearError();
        state.model = ev.model || null;
        state.free = ev.free; state.total = ev.total;
        state.entries = ev.entries; state.uploads = ev.uploads;
        applyState();
        renderWall();
        /* The first snapshot after a settled delete is the "after" reading.
           Rendered here rather than in `deleted` so both figures in the
           sentence are the device's own answers, taken one either side. */
        if (state.pendingDelete?.settled) {
          const pd = state.pendingDelete;
          state.pendingDelete = null;
          renderDeleteReport(pd, ev.free);
        }
        break;

      case 'scanned':
        state.root = ev.root;
        state.tracks = ev.tracks;
        state.encoder = ev.encoder;
        state.encoderVerified = ev.encoderVerified;
        state.focus = null;
        /* Prune the selection to what the scan still found, the way
           `renderWall` prunes against the live listing. A folder that changed
           underneath us must not leave a path in a set that Send would act
           on. */
        {
          const live = new Set(ev.tracks.map((t) => t.path));
          for (const p of [...libPicks]) if (!live.has(p)) libPicks.delete(p);
          lastLibIndex = null;
        }
        $('[data-count-all]').textContent = String(ev.found);
        $('[data-count-albums]').textContent = String(libraryGroups('albums').length);
        $('[data-count-artists]').textContent = String(libraryGroups('artists').length);
        applyLibraryView();
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
        /* A track the user asked to be renamed around a collision is on the
           watch under a name they did not type. Saying "sent" and stopping
           there would leave them looking for the wrong file. */
        if (ev.renamedTo) {
          addNotice('Sent under a new name',
            `${ev.name} is on your watch as ${ev.renamedTo}.`, null, 'info');
        }
        break;

      case 'fileSkipped':
        paintRun(ev);
        addNotice(SKIP_LABEL[ev.kind] || SKIP_LABEL.other, `${ev.name}: ${ev.reason}`);
        if (ev.kind === 'nameTaken' && state.run && ev.path) {
          state.run.nameTaken.push({ path: ev.path, name: ev.name });
        }
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

      case 'syncFinished':
        finishRun(ev);
        break;

      /* The listing found files the run had given up on. The observation
         beats the inference, and every reading of the sentence already on
         screen is revised rather than left standing beside a wall that
         contradicts it.

         "On screen" was the bug the first time. `finishRun` pins the meter's
         `aria-valuetext` to the run's own outcome and then turns this card's
         live region off, so a correction written only into the visible text
         would be seen and never heard — the stale, wrong sentence surviving
         for the one user least able to check it against the wall. Three
         things therefore happen here, not one: `outcomeHeading` now carries
         the correction so `applyState` repaints the heading with it, the
         meter's announced text is rewritten through `setProgress`, and the
         region is made live again for exactly as long as it takes to write
         the now-line. */
      case 'syncReconciled': {
        if (!state.run || !ev.landed.length) break;
        const run = state.run;
        run.landed = ev.landed;
        run.dismissed = false;
        const n = ev.landed.length;

        /* Same percentage. Reconciliation changes what the run *means*, not
           how many bytes crossed the cable — those were already counted, and
           A2's drained-but-unconfirmed case is precisely a file whose bytes
           all went. Only the sentence was wrong. */
        setProgress(run.pct || 0, outcomeHeading(run));

        transfer.setAttribute('aria-live', 'polite');
        $('[data-now-label]').textContent =
          `Reported ${run.failed} failed, but the watch is listing ` +
          `${n === 1 ? 'one of them' : n + ' of them'} — see the list below.`;
        addNotice('Found on your watch after all',
          `Pelican could not confirm ${n === 1 ? 'this file' : 'these files'} during the ` +
          `send, and the watch is now listing ${n === 1 ? 'it' : 'them'}: ` +
          ev.landed.join(', ') + '.', null, 'info');
        /* Off again on the next tick, the same pattern `finishRun` uses: the
           announcement is taken from the mutations above, and a report that
           has stopped changing must not go on claiming to be a live
           transfer. */
        setTimeout(() => {
          if (state.run === run && run.phase === 'finished') {
            transfer.setAttribute('aria-live', 'off');
          }
        }, 0);
        applyState();
        break;
      }

      case 'deleted':
        if (ev.failed === 0 && ev.ok > 0) clearError();
        /* The tallies are here; the free figure that closes the arithmetic is
           in the snapshot the engine sends next. Hold, do not render — a
           report that guessed the "after" number would be exactly the kind of
           prediction this element exists to replace. */
        if (state.pendingDelete) {
          state.pendingDelete.ok = ev.ok;
          state.pendingDelete.failed = ev.failed;
          state.pendingDelete.settled = true;
        }
        break;

      case 'deleteFailed':
        /* Collect, do not overwrite. One event arrives per refusal, and
           showError writes into a single box — with three refused stubs only
           the last reason survived while the report claimed all three were
           shown. The report is the right home: it has focus, and the reason
           belongs beside the count. */
        if (state.pendingDelete) {
          (state.pendingDelete.failures ||= []).push({ name: ev.name, error: ev.error });
        }
        showError('Could not delete that', `${ev.name}: ${ev.error}`);
        break;

      case 'coverArt': {
        const key = artPending.get(ev.path);
        artPending.delete(ev.path);
        if (key === undefined) break;
        /* `null` is cached too. It is an answer — this album has no embedded
           art — and re-asking on every track of it would re-read the file
           twelve times to be told the same thing. */
        artCache.set(key, ev.art || null);
        if (current && albumKey(current) === key) paintArt(ev.art || null);
        break;
      }

      case 'error':
        if (ev.contention) {
          state.connected = false;
          state.contention = ev.contention;
          applyState();
        } else if (state.run && state.run.phase === 'running') {
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

  const pickFolder = () => {
    clearError();
    invoke('pick_folder')
      .then((path) => { if (path) return invoke('scan_folder', { path }); })
      .catch((err) => showError('Could not open that folder', String(err)));
  };

  $('[data-pick]')?.addEventListener('click', (e) => {
    e.preventDefault();
    pickFolder();
  });

  $('[data-folder-act]')?.addEventListener('click', () => {
    /* No dialog on the rescan path: the shell already holds the root, and
       `scan_folder` takes exactly that. It is also the way out when the
       folder changed underneath the user. */
    if (!state.root) return pickFolder();
    clearError();
    invoke('scan_folder', { path: state.root })
      .catch((err) => showError('Could not read that folder again', String(err)));
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
      state.run = s === 'transferring'
        ? { phase: 'running', total: 5, bytes: 5e7, jobBytes: 1.7e7, startedAt: Date.now(),
            ok: 2, skipped: 0, failed: 0, stopped: false,
            delivered: 0, planned: 5e7, landed: [], dismissed: false }
        : null;
      applyState();
      renderWall();
    };
    window.addEventListener('hashchange', applyHash);
    applyHash();
  }

  applyState();
  applyLibraryView();
  updateSelection();
  /* Nothing is loaded yet, so the transport is a Play control. Left to the
     markup alone it shipped showing Pause, which told both the eye and a
     screen reader that something was playing. */
  if (playpause) setPlaying(playpause, false);
})();
