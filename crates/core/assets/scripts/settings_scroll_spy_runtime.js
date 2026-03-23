if (!window.__polySettingsScrollSpyRuntimeInit) {
    window.__polySettingsScrollSpyRuntimeInit = true;

    window.__polyScrollSettingsSectionById = function (id) {
        const el = document.getElementById(id);
        if (el) {
            el.scrollIntoView({ block: 'start', behavior: 'smooth' });
        }
    };

    window.__polyInstallSettingsScrollSpy = function (config) {
        if (!config || !config.runtimeFlag || !config.contentSelector || !Array.isArray(config.sectionIds) || config.sectionIds.length === 0) {
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

        const computeActiveSlug = function () {
            const content = document.querySelector(config.contentSelector);
            if (!content) {
                return null;
            }

            const contentRect = content.getBoundingClientRect();
            const threshold = contentRect.top + 24;
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
            dioxus.send(computeActiveSlug());
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
}
