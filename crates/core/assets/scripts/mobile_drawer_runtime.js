if (!window.__polyMobileDrawerInit) {
    window.__polyMobileDrawerInit = true;

    window.__polySetMobileRightWingOpen = function (open) {
        const root = document.querySelector('.poly-app');
        if (!root) {
            return;
        }

        root.classList.toggle('poly-mobile-right-wing-open', Boolean(open));
    };

    window.__polyToggleChatSideColumn = function () {
        const toggle = document.querySelector('.chat-members-toggle-btn');
        if (toggle instanceof HTMLElement) {
            toggle.click();
        }
    };

    window.__polyApplyMobileDrawerState = function (open) {
        const root = document.querySelector('.poly-app');
        if (!root) {
            return;
        }

        const favorites = document.querySelector('.server-sidebar');
        const account = document.querySelector('.account-server-bar');
        const panels = document.querySelectorAll('.poly-left-drawer-panel');

        if (favorites) {
            favorites.style.left = open ? '0px' : '-72px';
            favorites.style.right = 'auto';
        }

        if (account) {
            account.style.left = open ? '72px' : '-72px';
            account.style.right = 'auto';
        }

        panels.forEach(function (panel) {
            panel.style.left = 'auto';
            panel.style.right = open ? '0px' : '100vw';
        });
    };

    window.__polySyncMobileDrawerMode = function () {
        const root = document.querySelector('.poly-app');
        if (!root) {
            return;
        }

        const account = document.querySelector('.account-server-bar');
        const isMobileUi = root.classList.contains('poly-force-mobile') || window.innerWidth <= 640;
        if (!isMobileUi) {
            root.classList.remove('poly-mobile-drawer-open');
            root.classList.remove('poly-mobile-right-wing-open');
            root.style.removeProperty('--poly-mobile-rail-offset');
            window.__polyApplyMobileDrawerState(false);

            const favorites = document.querySelector('.server-sidebar');
            const account = document.querySelector('.account-server-bar');
            const panels = document.querySelectorAll('.poly-left-drawer-panel');
            if (favorites) {
                favorites.style.removeProperty('left');
            }
            if (account) {
                account.style.removeProperty('left');
            }
            panels.forEach(function (panel) {
                panel.style.removeProperty('right');
            });
            return;
        }

        const railOffset = 72 + (account ? 72 : 0);
        root.style.setProperty('--poly-mobile-rail-offset', `${railOffset}px`);
        window.__polyApplyMobileDrawerState(root.classList.contains('poly-mobile-drawer-open'));
    };

    window.__polySetMobileDrawerOpen = function (open) {
        const root = document.querySelector('.poly-app');
        if (!root) {
            return;
        }

        root.classList.toggle('poly-mobile-drawer-open', Boolean(open));
        window.__polyApplyMobileDrawerState(Boolean(open));
        window.__polySyncMobileDrawerMode?.();
    };

    let resizeFrame = null;
    window.addEventListener('resize', function () {
        if (resizeFrame !== null) {
            window.cancelAnimationFrame(resizeFrame);
        }

        resizeFrame = window.requestAnimationFrame(function () {
            resizeFrame = null;
            window.__polySyncMobileDrawerMode?.();
        });
    });

    window.__polySyncMobileDrawerMode?.();

    let tracking = null;

    document.addEventListener(
        'touchstart',
        function (event) {
            const root = document.querySelector('.poly-app');
            if (!root) {
                return;
            }

            const isMobileUi = root.classList.contains('poly-force-mobile') || window.innerWidth <= 640;
            if (!isMobileUi || !event.touches || event.touches.length !== 1) {
                tracking = null;
                return;
            }

            const touch = event.touches[0];
            const drawerOpen = root.classList.contains('poly-mobile-drawer-open');
            const x = touch.clientX;
            const y = touch.clientY;

            if (!drawerOpen && x <= 24) {
                tracking = { mode: 'open', startX: x, startY: y };
                return;
            }

            if (drawerOpen && x <= Math.min(window.innerWidth, 360)) {
                tracking = { mode: 'close', startX: x, startY: y };
                return;
            }

            const rightWingOpen = root.classList.contains('poly-mobile-right-wing-open');
            const canToggleRightWing = document.querySelector('.chat-members-toggle-btn');

            if (!rightWingOpen && canToggleRightWing && x >= window.innerWidth - 24) {
                tracking = { mode: 'open-right', startX: x, startY: y };
                return;
            }

            if (rightWingOpen && x >= Math.max(0, window.innerWidth - 360)) {
                tracking = { mode: 'close-right', startX: x, startY: y };
                return;
            }

            tracking = null;
        },
        { passive: true },
    );

    document.addEventListener(
        'touchend',
        function (event) {
            if (!tracking || !event.changedTouches || event.changedTouches.length !== 1) {
                tracking = null;
                return;
            }

            const root = document.querySelector('.poly-app');
            if (!root) {
                tracking = null;
                return;
            }

            const touch = event.changedTouches[0];
            const dx = touch.clientX - tracking.startX;
            const dy = Math.abs(touch.clientY - tracking.startY);

            if (dy <= 80) {
                if (tracking.mode === 'open' && dx >= 60) {
                    window.__polySetMobileDrawerOpen(true);
                } else if (tracking.mode === 'close' && dx <= -60) {
                    window.__polySetMobileDrawerOpen(false);
                } else if (tracking.mode === 'open-right' && dx <= -60) {
                    if (!root.classList.contains('poly-mobile-right-wing-open')) {
                        window.__polyToggleChatSideColumn?.();
                    }
                } else if (tracking.mode === 'close-right' && dx <= -60) {
                    window.__polySetMobileRightWingOpen(false);
                    window.setTimeout(function () {
                        if (root.classList.contains('poly-mobile-right-wing-open')) {
                            window.__polyToggleChatSideColumn?.();
                        }
                    }, 220);
                }
            }

            tracking = null;
        },
        { passive: true },
    );
}
