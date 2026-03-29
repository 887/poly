if (!window.__polyRightPanelResizerInit) {
    window.__polyRightPanelResizerInit = true;

    const MIN_WIDTH = 160;
    const MAX_WIDTH = 500;

    let dragging = false;

    document.addEventListener('pointerdown', function (e) {
        const handle = e.target.closest('.right-panel-resizer');
        if (!handle) {
            return;
        }

        dragging = true;
        try { handle.setPointerCapture(e.pointerId); } catch (_) {}
        document.body.style.cursor = 'col-resize';
        document.body.style.userSelect = 'none';
        e.preventDefault();
    }, true);

    document.addEventListener('pointermove', function (e) {
        if (!dragging) {
            return;
        }

        const width = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, window.innerWidth - e.clientX));
        document.documentElement.style.setProperty('--right-panel-width', width + 'px');
    }, true);

    document.addEventListener('pointerup', function () {
        if (!dragging) {
            return;
        }

        dragging = false;
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
    }, true);
}
