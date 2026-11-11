// Injected into music.youtube.com at document start by Nightjar YouTube.
//
// - Keeps the page's own media-session handlers (next, previous...) so commands use
//   YouTube's code paths rather than clicking page markup.
// - `__nj.sample()` returns the status as a JSON string; `__nj.cmd(name, arg)` runs a command.
//
// The spectrum comes from the app's Core Audio tap (AudioTap.swift), not Web Audio: WebKit
// doesn't feed Media Source streams into an AnalyserNode, so a graph here only reads silence.
(() => {
  if (window.__nj) return;
  const nj = (window.__nj = { handlers: {}, err: '' });

  // 1. Keep the handlers the page registers with the media session.
  try {
    const ms = navigator.mediaSession;
    const original = ms.setActionHandler.bind(ms);
    ms.setActionHandler = (action, handler) => {
      nj.handlers[action] = handler;
      return original(action, handler);
    };
  } catch (e) {
    nj.err = 'mediaSession: ' + e;
  }

  function video() {
    return document.querySelector('video');
  }

  nj.sample = () => {
    const v = video();
    const md = navigator.mediaSession && navigator.mediaSession.metadata;
    const playing = !!v && !v.paused && !v.ended;
    const state = !v || !v.src ? 'stopped' : playing ? 'playing' : 'paused';
    const art = md && md.artwork && md.artwork.length ? md.artwork[md.artwork.length - 1].src : '';
    const signedOut = !!document.querySelector('a[href*="accounts.google.com/ServiceLogin"], a.sign-in-link');
    return JSON.stringify({
      v: 1,
      state,
      title: (md && md.title) || '',
      artist: (md && md.artist) || '',
      album: (md && md.album) || '',
      art,
      pos: v ? Math.round(v.currentTime * 10) / 10 : 0,
      dur: v && isFinite(v.duration) ? Math.round(v.duration * 10) / 10 : 0,
      volume: v ? v.volume : 1,
      signedIn: !signedOut,
      page: location.host + location.pathname,
      err: nj.err,
    });
  };

  function click(selector) {
    const el = document.querySelector(selector);
    if (el) el.click();
    return !!el;
  }

  nj.cmd = (name, arg) => {
    const v = video();
    switch (name) {
      case 'toggle':
        if (click('#play-pause-button')) return;
        if (v) v.paused ? v.play() : v.pause();
        return;
      case 'next':
        if (nj.handlers.nexttrack) return nj.handlers.nexttrack({ action: 'nexttrack' });
        click('.next-button');
        return;
      case 'prev':
        if (nj.handlers.previoustrack) return nj.handlers.previoustrack({ action: 'previoustrack' });
        click('.previous-button');
        return;
      case 'seek':
        if (v && isFinite(arg)) v.currentTime = Math.max(0, arg);
        return;
      case 'volume':
        if (v && isFinite(arg)) v.volume = Math.min(1, Math.max(0, arg));
        return;
    }
  };
})();
