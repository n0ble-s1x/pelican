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

  let picks = [];

  const checked = () => picks.filter((p) => p.checked).map((p) => p.closest('tr'));

  /* Tracks that will transfer but stay invisible on the watch, because they
     carry no title or artist. Surfaced at selection time, not mid-send. */
  const untagged = () => checked().filter((r) => r.dataset.untagged === 'true').length;

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
      /* For MediaMetadata and for the album key the cover cache uses. The
         title falls back to the file stem in scan.rs, so it is never blank. */
      tr.dataset.title = t.title;
      tr.dataset.artist = t.artist || '';
      tr.dataset.album = t.album || '';
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
    buildQueue();
    publishMediaSession();
    updateSelection();
  }

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
    if (state.encoderVerified) {
      tag.textContent = `Converting with ${state.encoder}`;
      body.textContent =
        'Files your watch cannot play will be converted to 192 kbps MP3 on this Mac. ' +
        'This is the profile confirmed on a Forerunner 165.' +
        (refused ? ` ${plural(refused, 'file')} still cannot be converted at all.` : '');
      return;
    }
    /* Naming the untested path is the point — but the sentence has to name
       which half is untested, because the two halves now have different
       evidence behind them.

       Transfer is observed: afconvert-produced M4A files have landed on the
       attached FR165 and are listed by the device. What is still unverified
       is the other subsystem — `docs/macos-port.md` says it outright, that
       "upload acceptance and library indexing are different subsystems", and
       the MP3 profile is pinned to CBR 192 kbps precisely because the
       indexer is fussy. So `encoder_verified` stays false for afconvert and
       means *the watch plays it*, not *it arrives*.

       What the engine observed about ffmpeg is also narrower than "not
       installed": it tried to spawn `ffmpeg` off this process's PATH and
       failed, and a Finder-launched .app does not inherit the PATH a
       Homebrew ffmpeg lives on. Say the observation, not the inference. */
    tag.textContent = `Converting with ${state.encoder} — playback unconfirmed`;
    body.textContent =
      'Pelican could not find ffmpeg on its PATH, so conversions use macOS’s own ' +
      'afconvert and land as AAC in M4A. Files in this format have transferred to a ' +
      'Forerunner 165 successfully, but Pelican has not confirmed that the watch’s ' +
      'music app indexes and plays them — arriving and playing are different ' +
      'subsystems. An app launched from Finder does not see a Homebrew ffmpeg.' +
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
        sub: 'Your watch reports these by filename only. Pelican has no record '
           + 'of sending them, so it does not know their album or artist.',
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
      pick.setAttribute('aria-label', `Remove ${it.name} from your watch`);
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
    if (stubLink) {
      stubLink.hidden = brokenCount === 0;
      stubLink.textContent = brokenCount === 1
        ? 'select it' : `select all ${brokenCount}`;
      stubLink.setAttribute('aria-label', brokenCount === 1
        ? 'Select the unreadable file'
        : `Select all ${brokenCount} unreadable files`);
    }

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
       (docs/garmin-mtp.md §6). Hence "Ask", never "Delete". */
    $('[data-wall-delete]').textContent =
      p.files.length ? 'Delete from watch'
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
      body.append(
        strong(nameList(p.names(p.files))),
        text(' will be removed from your watch. Pelican cannot undo this, and '
          + 'the watch has no trash. Your copies on this Mac are not touched.'));
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
    confirmBox.hidden = true;
    /* Both post to the one device thread and are handled in order. The
       snapshot each produces is what closes the loop on screen; nothing here
       predicts the outcome. */
    if (paths.length) {
      invoke('delete_remote', { paths })
        .catch((e) => showError('Could not delete that', String(e)));
    }
    if (names.length) {
      invoke('forget_uploads', { names })
        .catch((e) => showError('Could not update Pelican’s record', String(e)));
    }
  }

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
      renderWall();
      closeConfirm(false);
      return;
    }
    const pick = e.target.closest('[data-pick]');
    if (!pick) return;
    const li = pick.closest('li');
    li.classList.toggle('is-picked', pick.checked);
    const key = pick.dataset.path || pick.dataset.ghost;
    const set = pick.dataset.path ? wallPicks : wallGhosts;
    if (pick.checked) set.add(key); else set.delete(key);
    /* A change to the selection invalidates the sentence the confirm is
       showing, so it closes rather than confirming a stale list. */
    closeConfirm(false);
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
    $$('.wall__list [data-pick], .wall__list [data-pick-group]').forEach((c) => {
      c.checked = false; c.indeterminate = false;
      c.closest('li').classList.remove('is-picked');
    });
    closeConfirm(false);
    updateWallbar();
  });

  $('[data-wall-delete]')?.addEventListener('click', openConfirm);
  $('[data-confirm-keep]')?.addEventListener('click', () => closeConfirm(true));
  $('[data-confirm-go]')?.addEventListener('click', runDelete);

  $('[data-pick-stubs]')?.addEventListener('click', () => {
    $$('.wall__list li[data-broken] [data-pick]').forEach((c) => {
      if (!c.checked) { c.checked = true; c.dispatchEvent(new Event('change', { bubbles: true })); }
    });
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
    queue = $$('.tracks tbody tr')
      .filter((r) => !$('.tracks__play .iconbtn', r).disabled);
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

  function startRun(total, bytes) {
    state.run = {
      phase: 'running', total, bytes, jobBytes: 0, startedAt: Date.now(),
      ok: 0, skipped: 0, failed: 0, stopped: false,
      delivered: 0, planned: bytes, landed: [], dismissed: false,
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
    /* The confirm outranks the report: Escape's first job is always to back
       out of the irreversible thing. */
    if (confirmBox && !confirmBox.hidden) { closeConfirm(true); return; }
    if (state.run && state.run.phase === 'finished' && !state.run.dismissed) {
      dismissRun();
    }
  });

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
        break;

      case 'scanned':
        state.root = ev.root;
        state.tracks = ev.tracks;
        state.encoder = ev.encoder;
        state.encoderVerified = ev.encoderVerified;
        state.focus = null;
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
          ev.landed.join(', ') + '.');
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
        break;

      case 'deleteFailed':
        /* Verbatim, same as fileFailed: the engine's message is the only
           thing that says whether a retry is safe. */
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

  $('[data-pick]')?.addEventListener('click', (e) => {
    e.preventDefault();
    clearError();
    invoke('pick_folder')
      .then((path) => { if (path) return invoke('scan_folder', { path }); })
      .catch((err) => showError('Could not open that folder', String(err)));
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
