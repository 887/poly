// Scroll-restoration runtime for Poly chat.
// Defines all window.poly* scroll helpers.
// Rules: no setTimeout. Use double-RAF when scrolling after a Dioxus signal write
// so RAF1 lets Dioxus commit its render before RAF2 applies the scroll.
(function () {
  if (window.__polyScrollRuntimeInit) return;
  window.__polyScrollRuntimeInit = true;

  window.__polyMessageScrollPositions =
    window.__polyMessageScrollPositions || Object.create(null);

  // Sequence counters — incremented on each new request so stale RAF callbacks
  // that fire after a newer request was issued are silently dropped.
  var _scrollSeq = 0; // used by polyScrollToBottom / polyRestoreScrollPosition
  var _posSeq = 0; // used by polyPreserveScrollDelta
  var _anchorSeq = 0; // used by polyPreserveMessageAnchor

  /** Scroll the message list to the very bottom on the next frame.
   * Uses scrollIntoView on the last message element rather than scrollTop=scrollHeight
   * because content-visibility:auto makes scrollHeight unreliable until off-screen
   * rows have been rendered (they use estimated intrinsic sizes before that). */
  window.polyScrollToBottom = function () {
    var seq = ++_scrollSeq;
    // Double-RAF: RAF1 lets Dioxus commit its render, RAF2 scrolls after layout.
    requestAnimationFrame(function () {
      requestAnimationFrame(function () {
        if (_scrollSeq !== seq) return;
        var el = document.getElementById("message-list-scroll");
        if (!el) return;
        var msgs = el.querySelectorAll('[id^="message-"]');
        var last = msgs[msgs.length - 1];
        if (last) { last.scrollIntoView({ block: "end" }); } else { el.scrollTop = el.scrollHeight; }
      });
    });
  };

  /**
   * Restore a previously remembered scroll position for channelId, or fall
   * back to the bottom if no position has been saved yet.
   */
  window.polyRestoreScrollPosition = function (channelId) {
    var seq = ++_scrollSeq;
    var saved = window.__polyMessageScrollPositions[channelId];
    if (Number.isFinite(saved)) {
      // Saved position: single RAF is fine — no new messages were injected.
      requestAnimationFrame(function () {
        if (_scrollSeq !== seq) return;
        var el = document.getElementById("message-list-scroll");
        if (!el) return;
        el.scrollTop = saved;
      });
    } else {
      // No saved position → fall back to bottom. Double-RAF so Dioxus renders first.
      requestAnimationFrame(function () {
        requestAnimationFrame(function () {
          if (_scrollSeq !== seq) return;
          var el = document.getElementById("message-list-scroll");
          if (!el) return;
          var msgs = el.querySelectorAll('[id^="message-"]');
          var last = msgs[msgs.length - 1];
          if (last) { last.scrollIntoView({ block: "end" }); } else { el.scrollTop = el.scrollHeight; }
        });
      });
    }
  };

  /**
   * Synchronously snapshot the current scrollTop for channelId.
   * No RAF — must capture the live value before any DOM mutation.
   */
  window.polyRememberScrollPosition = function (channelId) {
    var el = document.getElementById("message-list-scroll");
    if (el) window.__polyMessageScrollPositions[channelId] = el.scrollTop;
  };

  /**
   * After prepending/trimming messages, nudge scrollTop by deltaPx relative to
   * prevScrollTop so the same content stays under the user's eyes.
   */
  window.polyPreserveScrollDelta = function (prevScrollTop, deltaPx) {
    var seq = ++_posSeq;
    requestAnimationFrame(function () {
      if (_posSeq !== seq) return;
      var el = document.getElementById("message-list-scroll");
      if (!el) return;
      el.scrollTop = Math.max(0, prevScrollTop + deltaPx);
    });
  };

  /**
   * After a page swap, find anchorId and set scrollTop so the element sits at
   * exactly offsetPx from the top of the scroll container.
   */
  window.polyPreserveMessageAnchor = function (anchorId, offsetPx) {
    var seq = ++_anchorSeq;
    requestAnimationFrame(function () {
      if (_anchorSeq !== seq) return;
      var host = document.getElementById("message-list-scroll");
      var anchor = document.getElementById(anchorId);
      if (!host || !anchor) return;
      var hostRect = host.getBoundingClientRect();
      var anchorRect = anchor.getBoundingClientRect();
      var currentOffset = anchorRect.top - hostRect.top;
      host.scrollTop = Math.max(0, host.scrollTop + currentOffset - offsetPx);
    });
  };

  // ── Debug timing ─────────────────────────────────────────────────────────
  // Logs scroll pipeline latency to the browser console.
  // Measures: time from wheel input → scroll event, and scroll event spacing.
  // Remove or set window.__polyScrollDebugEnabled = false to silence.
  window.__polyScrollDebugEnabled = true;

  (function installScrollDebug() {
    var lastWheelTs = 0;
    var frameCount = 0;
    var lastFrameTs = 0;
    var slowFrames = 0;

    document.addEventListener(
      "wheel",
      function () {
        if (!window.__polyScrollDebugEnabled) return;
        lastWheelTs = performance.now();
      },
      { passive: true, capture: true }
    );

    var el = document.getElementById("message-list-scroll");
    if (el) attachScrollTimers(el);

    // Re-attach if the element is replaced (hot-reload).
    var _attachAttempts = 0;
    var _checkInterval = setInterval(function () {
      var newEl = document.getElementById("message-list-scroll");
      if (newEl && newEl !== el) {
        el = newEl;
        attachScrollTimers(el);
      }
      if (++_attachAttempts > 60) clearInterval(_checkInterval);
    }, 500);

    function attachScrollTimers(scrollEl) {
      scrollEl.addEventListener(
        "scroll",
        function () {
          if (!window.__polyScrollDebugEnabled) return;
          var now = performance.now();
          var sinceWheel = lastWheelTs > 0 ? (now - lastWheelTs).toFixed(1) : "n/a";
          var sinceLastFrame =
            lastFrameTs > 0 ? (now - lastFrameTs).toFixed(1) : "n/a";
          lastFrameTs = now;
          if (parseFloat(sinceLastFrame) > 50) {
            slowFrames++;
            console.warn(
              "[poly-scroll] SLOW FRAME #" +
                slowFrames +
                " — gap=" +
                sinceLastFrame +
                "ms  wheel→scroll=" +
                sinceWheel +
                "ms  msgs=" +
                scrollEl.querySelectorAll('[id^="message-"]').length
            );
          } else {
            if (frameCount % 30 === 0) {
              console.log(
                "[poly-scroll] ok — gap=" +
                  sinceLastFrame +
                  "ms  wheel→scroll=" +
                  sinceWheel +
                  "ms  msgs=" +
                  scrollEl.querySelectorAll('[id^="message-"]').length
              );
            }
          }
          frameCount++;
          lastWheelTs = 0;
        },
        { passive: true }
      );
    }
  })();
})();
