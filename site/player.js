// Dussek's website: one player for the page (a play button, a piece's name, a feature's step), the
// features' A/B (a step switched while playing keeps its place), the demos' cards travelling across
// the held picture, the download's system, the entrances. Without it the page still works: each play
// control is a link the browser plays, and the video and the download links are plain.
(function () {
  "use strict";
  document.documentElement.classList.add("js");
  var reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- the player
  var audio = new Audio();
  audio.preload = "none";
  var current = null; // the pressed control
  var bar = document.createElement("div");
  bar.className = "now";
  bar.setAttribute("role", "region");
  bar.setAttribute("aria-label", "Now playing");
  bar.innerHTML = '<span class="now-title"></span><span class="now-time">0:00</span><button type="button">Pause</button>';
  document.body.appendChild(bar);
  var barTitle = bar.querySelector(".now-title");
  var barTime = bar.querySelector(".now-time");
  var barButton = bar.querySelector("button");
  // out of the tab order and the reading until it first shows
  bar.inert = true;
  bar.setAttribute("aria-hidden", "true");
  barButton.tabIndex = -1;
  function showBar() {
    if (bar.classList.contains("on")) return;
    bar.classList.add("on");
    bar.inert = false;
    bar.removeAttribute("aria-hidden");
    barButton.removeAttribute("tabindex");
    // the page's end clear of it (the footer's links on a phone)
    document.documentElement.style.setProperty("--now-h", bar.offsetHeight + 16 + "px");
    document.documentElement.classList.add("playing");
  }
  // a step switched while playing seeks the new file once it loads: one pending time, so a quick
  // second choice does not inherit the first one's
  var seekTo = 0;
  audio.addEventListener("loadedmetadata", function () {
    if (seekTo > 0) audio.currentTime = Math.min(seekTo, Math.max(0, audio.duration - 0.05));
    seekTo = 0;
  });

  function mark(control, on) {
    if (control) control.setAttribute("aria-pressed", on ? "true" : "false");
  }
  function clock(t) {
    t = Math.max(0, Math.floor(t || 0));
    return Math.floor(t / 60) + ":" + String(t % 60).padStart(2, "0");
  }
  function play(control, keepTime) {
    if (!canOpus) { current = control; cannot("This browser cannot play the clips (Ogg Opus): a newer one can."); return; }
    var src = control.getAttribute("data-src");
    var at = keepTime ? audio.currentTime : 0;
    if (current === control && !audio.paused) {
      audio.pause();
      return;
    }
    if (current && current !== control) mark(current, false);
    current = control;
    if (audio.getAttribute("src") !== src) {
      seekTo = at;
      audio.src = src;
    }
    barTitle.textContent = control.getAttribute("data-title");
    showBar();
    audio.play().catch(function () {});
  }
  audio.addEventListener("play", function () { mark(current, true); barButton.textContent = "Pause"; });
  audio.addEventListener("pause", function () { mark(current, false); barButton.textContent = "Play"; });
  audio.addEventListener("ended", function () { mark(current, false); barButton.textContent = "Play"; });
  // the length once known (a server that does not say a file's size leaves it unknown while it streams)
  audio.addEventListener("timeupdate", function () {
    barTime.textContent = clock(audio.currentTime) + (isFinite(audio.duration) ? " / " + clock(audio.duration) : "");
  });
  // a browser without Ogg Opus (older Safari) says so instead of seeming to play
  var canOpus = audio.canPlayType('audio/ogg; codecs="opus"') !== "";
  function cannot(text) {
    mark(current, false);
    barTitle.textContent = text;
    barTime.textContent = "";
    barButton.textContent = "Play";
    showBar();
  }
  audio.addEventListener("error", function () {
    cannot(canOpus ? "This clip did not load." : "This browser cannot play the clips (Ogg Opus): a newer one can.");
  });
  barButton.addEventListener("click", function () {
    if (audio.paused) audio.play().catch(function () {}); else audio.pause();
  });

  document.addEventListener("click", function (e) {
    var b = e.target.closest("[data-src]");
    if (!b) return;
    e.preventDefault();
    // a feature's steps: the same phrase, so switching keeps the time
    var same = b.classList.contains("step") && current && current.classList.contains("step") &&
      current.closest(".feature") === b.closest(".feature") && current !== b && !audio.paused;
    play(b, same);
  });
  // the links to the clips become toggle buttons: pressed while theirs plays, Space plays as Enter does
  document.querySelectorAll("[data-src]").forEach(function (b) {
    b.setAttribute("role", "button");
    b.setAttribute("aria-pressed", "false");
  });
  document.addEventListener("keydown", function (e) {
    if (e.key !== " " || !e.target.closest) return;
    var b = e.target.closest("[data-src]");
    if (b) { e.preventDefault(); b.click(); }
  });
  // the video and the page's audio: one at a time
  document.querySelectorAll("video").forEach(function (v) {
    v.addEventListener("play", function () { audio.pause(); });
  });
  audio.addEventListener("play", function () {
    document.querySelectorAll("video").forEach(function (v) { v.pause(); });
  });

  // ---- the demos' cards across the held picture (desktop, motion allowed)
  var showcase = document.querySelector(".showcase");
  var track = showcase && showcase.querySelector(".cards");
  var wide = window.matchMedia("(min-width: 761px)");
  var focused = -1;
  function travel() {
    if (!track) return;
    if (reduce || !wide.matches) { track.style.removeProperty("--shift"); return; }
    // two whole cards in view at a time, stepping one card on as the page scrolls
    var r = showcase.getBoundingClientRect();
    var run = showcase.offsetHeight - window.innerHeight;
    var p = Math.min(1, Math.max(0, -r.top / Math.max(1, run)));
    var cards = track.children;
    if (cards.length < 3) return;
    var step = cards[1].offsetLeft - cards[0].offsetLeft;
    // the keyboard's card, while the focus is on one; else the scroll's
    var index = focused >= 0 ? Math.min(focused, cards.length - 2) : Math.round(p * (cards.length - 2));
    track.style.setProperty("--shift", (-index * step) + "px");
  }
  window.addEventListener("scroll", travel, { passive: true });
  window.addEventListener("resize", travel);
  travel();
  // a card reached by the keyboard comes into view, two whole cards as at rest; the focus scrolls the
  // clipped window by itself, so that scroll is taken back and the travel follows the focus instead
  if (track) {
    var win = track.parentElement;
    win.addEventListener("scroll", function () { if (win.scrollLeft !== 0) win.scrollLeft = 0; });
    track.addEventListener("focusin", function (e) {
      focused = Array.prototype.indexOf.call(track.children, e.target.closest(".card"));
      travel();
    });
    track.addEventListener("focusout", function (e) {
      if (!track.contains(e.relatedTarget)) { focused = -1; travel(); }
    });
  }

  // ---- the download: the visitor's system first
  var form = document.querySelector(".installer");
  if (form) {
    var go = form.querySelector(".installer-go");
    var note = form.querySelector(".installer-note");
    var base = form.getAttribute("action").replace(/[^/]+$/, "");
    function pick(input) {
      go.href = base + input.getAttribute("data-file");
      go.textContent = "Download for " + input.nextElementSibling.textContent;
      note.textContent = input.getAttribute("data-note");
    }
    form.addEventListener("change", function (e) { if (e.target.name === "os") pick(e.target); });
    form.addEventListener("submit", function (e) { e.preventDefault(); window.location = go.href; });
    var ua = navigator.userAgent;
    // a phone or a tablet (an iPad says "Macintosh" but has touch points): no system of its own to offer
    var handheld = /iPhone|iPad|iPod|Android/.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1);
    if (handheld) {
      form.querySelectorAll('input[name="os"]').forEach(function (i) { i.checked = false; });
      go.href = base.replace("/releases/download/", "/releases/tag/").replace(/\/$/, "");
      go.textContent = "All downloads";
      note.textContent = "Dussek runs on a computer (Windows, macOS or Linux), not on a phone or a tablet: choose the computer's system.";
    } else {
      var os = /Mac/.test(ua) ? "macos" : /Linux|X11/.test(ua) ? "linux" : "windows";
      var input = form.querySelector('input[value="' + os + '"]');
      if (input) { input.checked = true; pick(input); }
    }
  }

  // ---- entrances: each part of a section rising in, once
  if (!reduce && "IntersectionObserver" in window) {
    var parts = document.querySelectorAll(".field:not(.download) h2, .field .names, .field .work-note, .feature, .ensemble figure, .showcase-title");
    parts.forEach(function (el, i) {
      el.classList.add("rise");
      el.style.transitionDelay = (i % 4) * 75 + "ms";
    });
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (en) {
        if (en.isIntersecting) { en.target.classList.add("in"); io.unobserve(en.target); }
      });
    }, { rootMargin: "0px 0px -8% 0px" });
    parts.forEach(function (el) { io.observe(el); });
    // never left hidden: whatever is above the screen's bottom edge, and everything once the page's end is reached
    function catchUp() {
      var bottom = window.innerHeight;
      var end = window.scrollY + bottom >= document.documentElement.scrollHeight - 4;
      parts.forEach(function (el) {
        if (!el.classList.contains("in") && (end || el.getBoundingClientRect().top < bottom)) { el.classList.add("in"); io.unobserve(el); }
      });
    }
    window.addEventListener("scroll", catchUp, { passive: true });
    window.addEventListener("load", catchUp);
    window.addEventListener("resize", catchUp);
  }
})();
