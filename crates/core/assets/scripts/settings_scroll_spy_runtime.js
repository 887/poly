if (!window.__polySettingsScrollSpyRuntimeInit) {
    window.__polySettingsScrollSpyRuntimeInit = true;

    window.__polyScrollSettingsSectionById = function (id) {
        const el = document.getElementById(id);
        if (el) {
            el.scrollIntoView({ block: 'start', behavior: 'smooth' });
        }
    };

    window.__polySetSettingsNavActiveSlug = function (slug) {
        const items = document.querySelectorAll('.settings-nav [data-settings-slug]');
        for (const item of items) {
            const isActive = item.getAttribute('data-settings-slug') === slug;
            item.classList.toggle('active', isActive);
            item.setAttribute('aria-current', isActive ? 'page' : 'false');
        }
    };

    window.__polyInstallSettingsScrollSpy = function (config) {
            const scrollRootSelectors = Array.isArray(config?.scrollRootSelectors)
                ? config.scrollRootSelectors
                : Array.isArray(config?.scroll_root_selectors)
                    ? config.scroll_root_selectors
                    : [];

            if (!config || !config.runtimeFlag || scrollRootSelectors.length === 0 || !Array.isArray(config.sectionIds) || config.sectionIds.length === 0) {
            return 'missing';
        }

        if (window[config.runtimeFlag]) {
            return 'ready';
        }

        const resolveSlug = function (id) {
            if (config.pluginSectionPrefix && id.startsWith(config.pluginSectionPrefix)) {
                return `plugin-${id.slice(config.pluginSectionPrefix.length)}`;
            }

            if (config.sectionPrefix && id.startsWith(config.sectionPrefix)) {
                return id.slice(config.sectionPrefix.length);
            }

            return id;
        };

        const isMobileRuntimeActive = function () {
            const root = document.querySelector('.poly-app');
            return Boolean(root && root.classList.contains('poly-mobile-runtime-active'));
        };

        const orderedScrollRootSelectors = function () {
            const selectors = scrollRootSelectors.slice();
            const stageSelector = '.poly-split-content.settings-content > .poly-split-content-stage';
            const contentSelector = '.settings-content';

            if (isMobileRuntimeActive()) {
                return selectors.sort(function (a, b) {
                    if (a === stageSelector) return -1;
                    if (b === stageSelector) return 1;
                    if (a === contentSelector) return 1;
                    if (b === contentSelector) return -1;
                    return 0;
                });
            }

            return selectors.sort(function (a, b) {
                if (a === contentSelector) return -1;
                if (b === contentSelector) return 1;
                if (a === stageSelector) return 1;
                if (b === stageSelector) return -1;
                return 0;
            });
        };

        const resolveScrollRoot = function () {
            for (const selector of orderedScrollRootSelectors()) {
                const content = document.querySelector(selector);
                if (content) {
                    return content;
                }
            }

            return null;
        };

        const computeActiveSlug = function () {
            const content = resolveScrollRoot();
            if (!content) {
                return null;
            }

            const contentRect = content.getBoundingClientRect();
            // Sub-pixel layout and smooth-scrolling can leave section headers a
            // fraction of a pixel below the nominal top edge, so use a small
            // epsilon instead of a hard 24px cutoff.
            const threshold = contentRect.top + 32;
            let active = null;

            for (const id of config.sectionIds) {
                const el = document.getElementById(id);
                if (!el) {
                    continue;
                }

                const rect = el.getBoundingClientRect();
                if (rect.top <= threshold) {
                    active = resolveSlug(id);
                }
            }

            if (active) {
                return active;
            }

            return config.sectionIds.length > 0 ? resolveSlug(config.sectionIds[0]) : null;
        };

        let raf = 0;

        const emit = function () {
            raf = 0;
            const slug = computeActiveSlug();
            if (slug) {
                window.__polySetSettingsNavActiveSlug?.(slug);
                if (typeof dioxus !== 'undefined' && dioxus?.send) {
                    dioxus.send(slug);
                }
            }
        };

        const schedule = function () {
            if (raf) {
                return;
            }
            raf = requestAnimationFrame(emit);
        };

        const cleanup = function () {
            if (!window[config.runtimeFlag]) {
                return;
            }

            window[config.runtimeFlag] = false;
            document.removeEventListener('scroll', schedule, true);
            window.removeEventListener('resize', schedule);
            if (raf) {
                cancelAnimationFrame(raf);
                raf = 0;
            }
            delete window.__polySettingsScrollSpyCleanup;
        };

        window[config.runtimeFlag] = true;
        window.__polySettingsScrollSpyCleanup = cleanup;
        document.addEventListener('scroll', schedule, true);
        window.addEventListener('resize', schedule);
        schedule();

        return 'ready';
    };

    // Auto-install if pending config exists (runs on initial load and after hot-reload)
    if (window.__polySettingsScrollSpyPendingConfig) {
        const pendingConfig = window.__polySettingsScrollSpyPendingConfig;
        delete window.__polySettingsScrollSpyPendingConfig;
        window.__polyInstallSettingsScrollSpy(pendingConfig);
    }
}
