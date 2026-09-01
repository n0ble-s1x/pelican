/* Pelican — frontend behaviour. No framework, no bundler, no dependencies.
   This layer owns presentation only; every fact it shows will come from
   pelican-core through Tauri. Until that lands, the markup carries real
   sample values and the state is switchable from the URL hash so each
   state can be reviewed:  #transferring  #disconnected  #empty */

(() => {
  'use strict';

  const app = document.querySelector('.app');
  const transfer = document.querySelector('.transfer');
  const selection = document.querySelector('[data-selection]');
  const picks = [...document.querySelectorAll('.tracks__pick input')];
  const water = document.querySelector('.water');
  const pending = document.querySelector('.water__pending');
  const status = document.querySelector('.device__status');
  const sendWhy = document.querySelector('[data-send-why]');
  const prewarn = document.querySelector('.prewarn');
  const meter = document.querySelector('[data-meter]');

  /* Tracks that will transfer but stay invisible on the watch, because they
     carry no title or artist. Surfaced at selection time, not mid-send. */
  const untagged = () => picks
    .filter((p) => p.checked)
    .filter((p) => p.closest('tr').dataset.untagged === 'true').length;

  /* The water level is driven as a real height, not through a custom
     property: see the note in app.css — WKWebView will not transition a
     property whose value comes from one. */
  const setLevel = (el, pct) => { if (el) el.style.height = pct.toFixed(2) + '%'; };

  /* Capacity, in bytes, as the watch reports it. */
  const TOTAL = 3.71e9;
  const FREE = 2.41e9;

  /* Sizes come from the plan, not from the source file: a FLAC that will be
     converted lands far smaller than it starts. These are the converted
     estimates the planner produces. */
  const sizeOf = (row) => Number(row.dataset.bytes || 0);

  const fmt = (bytes) => {
    if (bytes >= 1e9) return (bytes / 1e9).toFixed(2) + ' GB';
    return Math.round(bytes / 1e6) + ' MB';
  };

  function updateSelection() {
    const rows = picks.filter((p) => p.checked).map((p) => p.closest('tr'));
    const bytes = rows.reduce((n, r) => n + sizeOf(r), 0);
    const fits = bytes <= FREE;

    /* The band sits on top of the current level and is measured against
       total capacity, so it answers "will this fit" directly. */
    if (pending) {
      const usedPct = (TOTAL - FREE) / TOTAL * 100;
      pending.style.bottom = usedPct.toFixed(2) + '%';
      pending.style.height = (bytes / TOTAL * 100).toFixed(2) + '%';
    }

    if (!selection) return;
    if (!rows.length) {
      selection.textContent = 'Nothing selected';
      selection.removeAttribute('data-fits');
    } else if (fits) {
      const after = FREE - bytes;
      selection.textContent =
        `${rows.length} selected · ${fmt(bytes)} · ${fmt(after)} would remain`;
      selection.dataset.fits = 'yes';
    } else {
      selection.textContent =
        `${rows.length} selected · ${fmt(bytes)} · ${fmt(bytes - FREE)} too much`;
      selection.dataset.fits = 'no';
    }

    const send = document.querySelector('[data-send]');
    if (send) {
      send.textContent = rows.length
        ? `Send ${rows.length} track${rows.length === 1 ? '' : 's'}` : 'Send';
      send.disabled = !rows.length || !fits || app.dataset.device === 'none';
    }

    /* A disabled control that does not say why is a dead end. The reason is
       rendered in the DOM and referenced by aria-describedby, so it reaches
       pointer and screen reader alike. */
    if (sendWhy) {
      let why = '';
      if (app.dataset.device === 'none') why = 'Connect your watch to send music.';
      else if (!rows.length) why = 'Tick a track to send it.';
      else if (!fits) why = `That is ${fmt(bytes - FREE)} more than your watch has room for.`;
      sendWhy.textContent = why;
      sendWhy.hidden = !why;
    }

    if (prewarn) {
      const n = untagged();
      prewarn.hidden = n === 0;
      if (n) {
        const tag = prewarn.querySelector('.prewarn__tag');
        if (tag) tag.textContent =
          `${n} track${n === 1 ? '' : 's'} need${n === 1 ? 's' : ''} a title`;
      }
    }
  }

  picks.forEach((p) => p.addEventListener('change', updateSelection));

  /* Play / pause. Real playback is an <audio> element the webview decodes
     natively — MP3, AAC, ALAC, FLAC and WAV all without a library. */
  const setIcon = (btn, name) => {
    const use = btn.querySelector('use');
    if (use) use.setAttribute('href', '#i-' + name);
  };

  const playpause = document.querySelector('.playpause');

  function setPlaying(btn, playing, label) {
    btn.setAttribute('aria-pressed', String(playing));
    btn.setAttribute('aria-label', (playing ? 'Pause' : 'Play') + (label ? ' ' + label : ''));
    setIcon(btn, playing ? 'pause' : 'play');
  }

  if (playpause) {
    playpause.addEventListener('click', () => {
      setPlaying(playpause, playpause.getAttribute('aria-pressed') !== 'true');
    });
  }

  /* Every row's play control was inert markup until now. */
  document.querySelectorAll('.tracks__play .iconbtn').forEach((btn) => {
    btn.addEventListener('click', () => {
      const row = btn.closest('tr');
      const title = row.querySelector('.t')?.textContent.trim() || '';
      const wasPlaying = btn.getAttribute('aria-pressed') === 'true';

      document.querySelectorAll('.tracks__play .iconbtn').forEach((other) => {
        const r = other.closest('tr');
        r.classList.remove('is-playing');
        setPlaying(other, false, r.querySelector('.t')?.textContent.trim() || '');
      });

      if (!wasPlaying) {
        row.classList.add('is-playing');
        setPlaying(btn, true, title);
        const np = document.querySelector('.player__meta .t');
        if (np) np.textContent = title;
        if (playpause) setPlaying(playpause, true);
      } else if (playpause) {
        setPlaying(playpause, false);
      }
    });
  });

  /* The progress bar reported a static 43% to assistive tech for the whole
     transfer. Keep the value and the bar in step from one place. */
  function setProgress(pct) {
    if (!meter) return;
    meter.setAttribute('aria-valuenow', String(Math.round(pct)));
    const fill = meter.querySelector('i');
    if (fill) fill.style.transform = `scaleX(${(pct / 100).toFixed(3)})`;
  }
  setProgress(43);

  /* Seek: keep the filled track in step with the thumb. */
  const seek = document.querySelector('.seek');
  if (seek) {
    const paint = () => {
      const pct = (seek.value / seek.max) * 100;
      seek.style.background =
        `linear-gradient(90deg, var(--ink-dim) ${pct}%, rgba(226,236,247,.10) ${pct}%)`;
    };
    seek.addEventListener('input', paint);
    paint();
  }

  /* The send gesture: rows sink toward the water, the water rises to meet
     them. Honours prefers-reduced-motion by skipping straight to the end. */
  const reduced = matchMedia('(prefers-reduced-motion: reduce)');

  function send() {
    const rows = picks.filter((p) => p.checked).map((p) => p.closest('tr'));
    if (!rows.length) return;
    const added = rows.reduce((n, r) => n + sizeOf(r), 0);

    const settle = () => {
      rows.forEach((r) => { r.querySelector('input').checked = false; });
      setLevel(water, (TOTAL - FREE + added) / TOTAL * 100);
      updateSelection();
    };

    if (reduced.matches) { settle(); return; }
    rows.forEach((r) => r.classList.add('is-sending'));
    setTimeout(() => {
      rows.forEach((r) => r.classList.remove('is-sending'));
      settle();
    }, 260);
  }

  const sendBtn = document.querySelector('.btn--primary');
  if (sendBtn) sendBtn.addEventListener('click', send);

  /* State review, from the URL hash. Replaced by real device events. */

  function applyState() {
    const s = location.hash.replace('#', '');
    const connected = s !== 'disconnected';
    app.dataset.device = connected ? 'connected' : 'none';

    /* Set the status in the DOM rather than through CSS content: generated
       text is not reliably announced, and this string is the answer to the
       first question a user asks. */
    if (status) status.textContent = connected ? 'connected' : 'no watch found';

    if (transfer) transfer.hidden = s !== 'transferring';
    updateSelection();
  }
  window.addEventListener('hashchange', applyState);

  applyState();
  updateSelection();
})();
