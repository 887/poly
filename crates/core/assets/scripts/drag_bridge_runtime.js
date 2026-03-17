if (!window.__polyDragInit) {
    window.__polyDragInit = true;

    document.addEventListener(
        'dragstart',
        function (event) {
            if (!event.dataTransfer) {
                return;
            }

            try {
                event.dataTransfer.setData('text/plain', 'poly-drag');
            } catch (_error) {
                // WebKit may throw for unsupported drag payloads; best effort only.
            }
        },
        true,
    );

    document.addEventListener(
        'dragover',
        function (event) {
            event.preventDefault();
        },
        true,
    );

    document.addEventListener(
        'drop',
        function (event) {
            event.preventDefault();
        },
        true,
    );
}
