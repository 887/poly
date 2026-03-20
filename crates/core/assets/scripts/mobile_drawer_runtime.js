if (!window.__polyMobileDrawerInit) {
    window.__polyMobileDrawerInit = true;

    const MOBILE_CLASS = 'poly-mobile-runtime-active';
    const LEFT_OPEN_CLASS = 'poly-mobile-left-wing-open';
    const RIGHT_OPEN_CLASS = 'poly-mobile-right-wing-open';
    const LEFT_DRAGGING_CLASS = 'poly-mobile-left-wing-dragging';
    const RIGHT_DRAGGING_CLASS = 'poly-mobile-right-wing-dragging';
    const SNAP_THRESHOLD = 0.2;

    function getRoot() {
        return document.querySelector('.poly-app');
    }

    function clamp(value, min, max) {
        return Math.min(max, Math.max(min, value));
    }

    function isMobileUi(root) {
        if (!root) {
            return false;
        }

        if (root.classList.contains('poly-layout-mode-force-mobile')) {
            return true;
        }

        if (root.classList.contains('poly-layout-mode-force-desktop')) {
            return false;
        }

        if (root.classList.contains('poly-layout-mode-auto-portrait')) {
            return window.innerHeight > window.innerWidth;
        }

        return window.innerWidth <= 640;
    }

    function isMirrored(root) {
        return Boolean(root) && root.classList.contains('poly-menu-mirrored');
    }

    function railOffsetPx() {
        return document.querySelector('.account-server-bar') ? 144 : 72;
    }

    function computeLeftRevealPx() {
        return Math.min(window.innerWidth * 0.9, 420);
    }

    function computeRightRevealPx() {
        return Math.min(window.innerWidth * 0.9, 360);
    }

    function cssNumber(root, propertyName, fallback) {
        const raw = window.getComputedStyle(root).getPropertyValue(propertyName);
        const parsed = Number.parseFloat(raw);
        return Number.isFinite(parsed) ? parsed : fallback;
    }

    function leftRevealPx(root) {
        return cssNumber(root, '--poly-mobile-left-reveal-px', window.innerWidth * 0.9);
    }

    function rightRevealPx(root) {
        return cssNumber(root, '--poly-mobile-right-reveal-px', window.innerWidth * 0.9);
    }

    function applyStageTransforms(root) {
        const leftOffset = cssNumber(root, '--poly-mobile-left-offset-px', 0);
        const rightOffset = cssNumber(root, '--poly-mobile-right-offset-px', 0);
        const mirrored = isMirrored(root);

        document.querySelectorAll('.poly-split-content').forEach(function (element) {
            if (element instanceof HTMLElement) {
                if (mirrored) {
                    element.style.removeProperty('left');
                    element.style.right = `${leftOffset}px`;
                } else {
                    element.style.removeProperty('right');
                    element.style.left = `${leftOffset}px`;
                }
            }
        });

        document.querySelectorAll('.chat-main-column').forEach(function (element) {
            if (element instanceof HTMLElement) {
                if (mirrored) {
                    element.style.removeProperty('left');
                    element.style.right = `${rightOffset}px`;
                } else {
                    element.style.removeProperty('right');
                    element.style.left = `${rightOffset}px`;
                }
            }
        });

        document.querySelectorAll('.chat-side-column').forEach(function (element) {
            if (element instanceof HTMLElement) {
                element.style.removeProperty('left');
                element.style.removeProperty('right');
            }
        });
    }

    function setLeftProgress(root, progress) {
        const next = clamp(progress, 0, 1);
        root.style.setProperty('--poly-mobile-left-progress', `${next}`);
        root.style.setProperty('--poly-mobile-left-offset-px', `${next * leftRevealPx(root)}px`);
        root.classList.toggle(LEFT_OPEN_CLASS, next >= 1);
        applyStageTransforms(root);
    }

    function setRightProgress(root, progress) {
        const next = clamp(progress, 0, 1);
        const reveal = rightRevealPx(root);
        root.style.setProperty('--poly-mobile-right-progress', `${next}`);
        root.style.setProperty('--poly-mobile-right-offset-px', `${-1 * next * reveal}px`);
        root.style.setProperty('--poly-mobile-right-panel-offset-px', `${-1 * (1 - next) * reveal}px`);
        root.classList.toggle(RIGHT_OPEN_CLASS, next >= 1);
        applyStageTransforms(root);
    }

    window.__polyToggleChatSideColumn = function () {
        const toggle = document.querySelector('.chat-members-toggle-btn');
        if (toggle instanceof HTMLElement) {
            toggle.click();
        }
    };

    window.__polyRequestOpenMobileRightWing = function () {
        const root = getRoot();
        if (!root) {
            return;
        }

        window.__polySetMobileDrawerOpen?.(false);
        const toggle = document.querySelector('.chat-members-toggle-btn');
        if (toggle instanceof HTMLElement) {
            toggle.click();
        }
        setRightProgress(root, 1);
    };

    window.__polyRequestCloseMobileRightWing = function () {
        const root = getRoot();
        if (!root) {
            return;
        }

        const explicitClose = document.querySelector('.poly-mobile-right-wing-close-state');
        if (explicitClose instanceof HTMLElement) {
            explicitClose.click();
            setRightProgress(root, 0);
            return;
        }

        const toggle = document.querySelector('.chat-members-toggle-btn');
        if (toggle instanceof HTMLElement) {
            toggle.click();
        }
        setRightProgress(root, 0);
    };

    window.__polySetMobileRightWingOpen = function (open) {
        const root = getRoot();
        if (!root) {
            return;
        }

        if (open) {
            setLeftProgress(root, 0);
        }
        setRightProgress(root, open ? 1 : 0);
    };

    window.__polySetMobileDrawerOpen = function (open) {
        const root = getRoot();
        if (!root) {
            return;
        }

        if (open) {
            setRightProgress(root, 0);
        }
        setLeftProgress(root, open ? 1 : 0);
        window.__polySyncMobileDrawerMode?.();
    };

    window.__polyToggleMobileDrawerOpen = function () {
        const root = getRoot();
        if (!root) {
            return;
        }

        window.__polySetMobileDrawerOpen(!root.classList.contains(LEFT_OPEN_CLASS));
    };

    window.__polySyncMobileDrawerMode = function () {
        const root = getRoot();
        if (!root) {
            return;
        }

        const wasMobileActive = root.classList.contains(MOBILE_CLASS);
        const mobileActive = isMobileUi(root);
        root.classList.toggle(MOBILE_CLASS, mobileActive);
        if (!mobileActive) {
            root.classList.remove(LEFT_OPEN_CLASS, RIGHT_OPEN_CLASS, LEFT_DRAGGING_CLASS, RIGHT_DRAGGING_CLASS);
            root.style.removeProperty('--poly-mobile-rail-offset');
            root.style.removeProperty('--poly-mobile-left-progress');
            root.style.removeProperty('--poly-mobile-right-progress');
            root.style.removeProperty('--poly-mobile-left-offset-px');
            root.style.removeProperty('--poly-mobile-right-offset-px');
            document.querySelectorAll('.poly-split-content, .chat-main-column').forEach(function (element) {
                if (element instanceof HTMLElement) {
                    element.style.removeProperty('left');
                    element.style.removeProperty('right');
                }
            });
            document.querySelectorAll('.chat-side-column').forEach(function (element) {
                if (element instanceof HTMLElement) {
                    element.style.removeProperty('left');
                    element.style.removeProperty('right');
                }
            });
            return;
        }

        root.style.setProperty('--poly-mobile-rail-offset', `${railOffsetPx()}px`);
        root.style.setProperty('--poly-mobile-left-reveal-px', `${computeLeftRevealPx()}px`);
        root.style.setProperty('--poly-mobile-right-reveal-px', `${computeRightRevealPx()}px`);
        if (!wasMobileActive) {
            window.__polyRequestCloseMobileRightWing?.();
        }
        setLeftProgress(root, root.classList.contains(LEFT_OPEN_CLASS) ? 1 : 0);
        setRightProgress(root, root.classList.contains(RIGHT_OPEN_CLASS) ? 1 : 0);
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
            const root = getRoot();
            if (!root || !isMobileUi(root) || !event.touches || event.touches.length !== 1) {
                tracking = null;
                return;
            }

            const touch = event.touches[0];
            const x = touch.clientX;
            const y = touch.clientY;
            const mirrored = isMirrored(root);
            const leftOpen = root.classList.contains(LEFT_OPEN_CLASS);
            const rightOpen = root.classList.contains(RIGHT_OPEN_CLASS);
            const canOpenLeft = Boolean(document.querySelector('.poly-left-drawer-panel'));
            const canOpenRight = Boolean(document.querySelector('.chat-side-column') || document.querySelector('.chat-members-toggle-btn'));

            const leftOpenEdgeHit = mirrored
                ? x >= window.innerWidth - leftRevealPx(root) - 24
                : x <= leftRevealPx(root) + 24;
            const leftClosedEdgeHit = mirrored
                ? x >= window.innerWidth - 24
                : x <= 24;

            if ((leftOpen && leftOpenEdgeHit) || (!leftOpen && canOpenLeft && leftClosedEdgeHit)) {
                tracking = {
                    side: 'left',
                    startX: x,
                    startY: y,
                    startProgress: cssNumber(root, '--poly-mobile-left-progress', leftOpen ? 1 : 0),
                    reveal: leftRevealPx(root),
                    dragging: false,
                };
                return;
            }

            const rightOpenEdgeHit = mirrored
                ? x <= rightRevealPx(root) + 24
                : x >= window.innerWidth - rightRevealPx(root) - 24;
            const rightClosedEdgeHit = mirrored
                ? x <= 24
                : x >= window.innerWidth - 24;

            if ((rightOpen && rightOpenEdgeHit) || (!rightOpen && canOpenRight && rightClosedEdgeHit)) {
                tracking = {
                    side: 'right',
                    startX: x,
                    startY: y,
                    startProgress: cssNumber(root, '--poly-mobile-right-progress', rightOpen ? 1 : 0),
                    reveal: rightRevealPx(root),
                    dragging: false,
                };
                return;
            }

            tracking = null;
        },
        { passive: true },
    );

    document.addEventListener(
        'touchmove',
        function (event) {
            const root = getRoot();
            if (!root || !tracking || !event.touches || event.touches.length !== 1) {
                return;
            }

            const touch = event.touches[0];
            const dx = touch.clientX - tracking.startX;
            const dy = Math.abs(touch.clientY - tracking.startY);

            if (!tracking.dragging) {
                if (Math.abs(dx) < 8) {
                    return;
                }
                if (Math.abs(dx) < dy) {
                    tracking = null;
                    return;
                }
                tracking.dragging = true;
            }

            event.preventDefault();
            const mirrored = isMirrored(root);

            if (tracking.side === 'left') {
                root.classList.add(LEFT_DRAGGING_CLASS);
                setLeftProgress(root, tracking.startProgress + ((mirrored ? -1 : 1) * dx) / tracking.reveal);
                setRightProgress(root, 0);
            } else {
                root.classList.add(RIGHT_DRAGGING_CLASS);
                setRightProgress(root, tracking.startProgress + ((mirrored ? 1 : -1) * dx) / tracking.reveal);
                setLeftProgress(root, 0);
            }
        },
        { passive: false },
    );

    function finishTracking(cancelled) {
        const root = getRoot();
        if (!root || !tracking) {
            tracking = null;
            return;
        }

        const side = tracking.side;
        const finalProgress = side === 'left'
            ? cssNumber(root, '--poly-mobile-left-progress', 0)
            : cssNumber(root, '--poly-mobile-right-progress', 0);
        const open = !cancelled && finalProgress >= SNAP_THRESHOLD;

        root.classList.remove(LEFT_DRAGGING_CLASS, RIGHT_DRAGGING_CLASS);

        if (side === 'left') {
            window.__polySetMobileDrawerOpen?.(open);
        } else if (open) {
            window.__polyRequestOpenMobileRightWing?.();
        } else {
            window.__polyRequestCloseMobileRightWing?.();
        }

        tracking = null;
    }

    document.addEventListener(
        'touchend',
        function () {
            finishTracking(false);
        },
        { passive: true },
    );

    document.addEventListener(
        'touchcancel',
        function () {
            finishTracking(true);
        },
        { passive: true },
    );
}
