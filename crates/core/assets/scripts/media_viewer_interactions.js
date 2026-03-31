// Media viewer interaction handler.
//
// Loaded via include_str! + document::eval() so the `dioxus` bridge is
// available for sending messages back to Rust.
//
// Messages sent to Rust:
//   "esc"          — Escape key pressed → go back
//   "prev"         — ArrowLeft key pressed → previous image
//   "next"         — ArrowRight key pressed → next image
//   "zf:<factor>"  — zoom factor (multiplicative, e.g. "zf:1.12" or "zf:0.8929")
//   "dp:<dx>:<dy>" — drag/pan delta in screen pixels (e.g. "dp:10:-5")

(function () {
    // ---- Keyboard navigation ----

    function onKey(e) {
        if (e.key === 'Escape') {
            document.removeEventListener('keydown', onKey, true);
            dioxus.send('esc');
        } else if (e.key === 'ArrowLeft') {
            dioxus.send('prev');
        } else if (e.key === 'ArrowRight') {
            dioxus.send('next');
        }
    }

    document.addEventListener('keydown', onKey, true);

    // ---- Locate stage element ----

    var st = document.querySelector('.poly-media-viewer-stage');
    if (!st) { return; }

    // ---- Mouse-wheel zoom ----

    st.addEventListener(
        'wheel',
        function (e) {
            e.preventDefault();
            // 1.12 ≈ 12% zoom-in per tick; 0.8929 ≈ 1/1.12 zoom-out per tick
            dioxus.send('zf:' + (e.deltaY < 0 ? '1.12' : '0.8929'));
        },
        { passive: false },
    );

    // ---- Touch: pinch-zoom + single-finger pan ----

    // pd  — previous pinch distance (null when no pinch active)
    // lx/ly — last touch position for single-finger pan
    var pd = null;
    var lx = null;
    var ly = null;

    function pinchDist(touches) {
        var dx = touches[0].clientX - touches[1].clientX;
        var dy = touches[0].clientY - touches[1].clientY;
        return Math.hypot(dx, dy);
    }

    st.addEventListener(
        'touchstart',
        function (e) {
            if (e.touches.length === 2) {
                pd = pinchDist(e.touches);
                lx = null;
                ly = null;
            } else if (e.touches.length === 1) {
                lx = e.touches[0].clientX;
                ly = e.touches[0].clientY;
            }
        },
        { passive: true },
    );

    st.addEventListener(
        'touchmove',
        function (e) {
            e.preventDefault();
            if (e.touches.length === 2 && pd !== null) {
                var d = pinchDist(e.touches);
                dioxus.send('zf:' + (d / pd).toFixed(4));
                pd = d;
            } else if (e.touches.length === 1 && lx !== null) {
                var dx = e.touches[0].clientX - lx;
                var dy = e.touches[0].clientY - ly;
                lx = e.touches[0].clientX;
                ly = e.touches[0].clientY;
                if (dx || dy) {
                    dioxus.send('dp:' + Math.round(dx) + ':' + Math.round(dy));
                }
            }
        },
        { passive: false },
    );

    st.addEventListener(
        'touchend',
        function (e) {
            pd = null;
            if (e.touches.length === 0) {
                lx = null;
                ly = null;
            } else if (e.touches.length === 1) {
                lx = e.touches[0].clientX;
                ly = e.touches[0].clientY;
            }
        },
        { passive: true },
    );

    // ---- Mouse drag pan ----

    // dr    — currently dragging
    // moved — dragged at least one pixel (suppresses backdrop-dismiss click)
    var dr = false;
    var moved = false;

    st.addEventListener('mousedown', function (e) {
        if (e.button !== 0) { return; }
        dr = true;
        moved = false;
        lx = e.clientX;
        ly = e.clientY;
        st.style.cursor = 'grabbing';
        e.preventDefault();
    });

    document.addEventListener('mousemove', function (e) {
        if (!dr) { return; }
        var dx = e.clientX - lx;
        var dy = e.clientY - ly;
        lx = e.clientX;
        ly = e.clientY;
        if (dx || dy) {
            moved = true;
            dioxus.send('dp:' + Math.round(dx) + ':' + Math.round(dy));
        }
    });

    document.addEventListener('mouseup', function () {
        if (dr) {
            dr = false;
            st.style.cursor = '';
        }
    });

    // Suppress backdrop-dismiss click if the user was dragging.
    st.addEventListener('click', function (e) {
        if (moved) {
            e.stopPropagation();
            moved = false;
        }
    });
})();
