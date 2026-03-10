# Knowledge: Sidebar footer layout fix 2026-03-10

*Last Updated: 2026-03-10T22:45:08.171289799+00:00*

---

The .sidebar-footer in FavoritesBar was position:absolute which overlapped .sidebar-scroll-area making scrollbar hidden. Fixed in tailwind.css by removing position:absolute and making it a flex-shrink:0 flex child. Now scrollArea.bottom (567.5) and footerTop (575.5) are properly separated.
