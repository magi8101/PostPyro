// Mobile menu, nav-link scroll spy, quick-start tabs, copy-to-clipboard,
// API sidebar scroll spy, scroll-to-top, and a basic in-page text search.
document.addEventListener('DOMContentLoaded', function () {
    // Mobile hamburger menu
    const hamburger = document.querySelector('.hamburger');
    const navMenu = document.querySelector('.nav-menu');

    if (hamburger && navMenu) {
        hamburger.addEventListener('click', function () {
            navMenu.classList.toggle('active');
            hamburger.classList.toggle('active');
        });
    }

    // Smooth scrolling + active state for top nav links
    const navLinks = document.querySelectorAll('.nav-link[href^="#"]');

    function setActiveNavLink(targetId) {
        navLinks.forEach(function (link) {
            link.classList.toggle('active', link.getAttribute('href') === targetId);
        });
    }

    navLinks.forEach(function (link) {
        link.addEventListener('click', function (e) {
            e.preventDefault();
            const targetId = this.getAttribute('href');
            const targetSection = document.querySelector(targetId);
            if (!targetSection) return;

            window.scrollTo({
                top: targetSection.offsetTop - 72,
                behavior: 'smooth',
            });

            if (navMenu) navMenu.classList.remove('active');
            if (hamburger) hamburger.classList.remove('active');
            setActiveNavLink(targetId);
        });
    });

    const sectionObserver = new IntersectionObserver(
        function (entries) {
            entries.forEach(function (entry) {
                if (entry.isIntersecting) setActiveNavLink('#' + entry.target.id);
            });
        },
        { rootMargin: '-80px 0px -50% 0px', threshold: 0 }
    );
    document.querySelectorAll('section[id]').forEach(function (section) {
        sectionObserver.observe(section);
    });

    // Quick-start tabs
    const tabButtons = document.querySelectorAll('.tab-btn');
    tabButtons.forEach(function (btn) {
        btn.addEventListener('click', function () {
            const tabName = btn.getAttribute('data-tab');

            document.querySelectorAll('.tab-content').forEach(function (content) {
                content.classList.remove('active');
            });
            tabButtons.forEach(function (b) {
                b.classList.remove('active');
            });

            const selected = document.getElementById('tab-' + tabName);
            if (selected) selected.classList.add('active');
            btn.classList.add('active');

            if (typeof Prism !== 'undefined') Prism.highlightAll();
        });
    });

    // Copy-to-clipboard buttons
    document.querySelectorAll('.copy-btn').forEach(function (btn) {
        const text = btn.getAttribute('data-copy') || '';
        btn.addEventListener('click', function () {
            const done = function () {
                const original = btn.innerHTML;
                btn.innerHTML =
                    '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>';
                btn.style.color = 'var(--success)';
                setTimeout(function () {
                    btn.innerHTML = original;
                    btn.style.color = '';
                }, 1600);
            };

            if (navigator.clipboard && window.isSecureContext) {
                navigator.clipboard.writeText(text).then(done).catch(function () {
                    fallbackCopy(text, done);
                });
            } else {
                fallbackCopy(text, done);
            }
        });
    });

    function fallbackCopy(text, done) {
        const textarea = document.createElement('textarea');
        textarea.value = text;
        textarea.style.position = 'fixed';
        textarea.style.left = '-9999px';
        document.body.appendChild(textarea);
        textarea.select();
        try {
            document.execCommand('copy');
            done();
        } catch (err) {
            console.error('Copy failed:', err);
        }
        document.body.removeChild(textarea);
    }

    // API reference sidebar
    const apiLinks = document.querySelectorAll('.api-link');
    apiLinks.forEach(function (link) {
        link.addEventListener('click', function (e) {
            e.preventDefault();
            const targetId = this.getAttribute('href');
            const targetSection = document.querySelector(targetId);
            if (!targetSection) return;

            window.scrollTo({ top: targetSection.offsetTop - 88, behavior: 'smooth' });
            apiLinks.forEach(function (l) {
                l.classList.remove('active');
            });
            this.classList.add('active');
        });
    });

    const apiObserver = new IntersectionObserver(
        function (entries) {
            entries.forEach(function (entry) {
                if (!entry.isIntersecting) return;
                const id = '#' + entry.target.id;
                apiLinks.forEach(function (link) {
                    link.classList.toggle('active', link.getAttribute('href') === id);
                });
            });
        },
        { rootMargin: '-100px 0px -50% 0px', threshold: 0 }
    );
    document.querySelectorAll('.api-section').forEach(function (section) {
        apiObserver.observe(section);
    });

    // Scroll-to-top button
    const scrollTopBtn = document.getElementById('scroll-top');
    if (scrollTopBtn) {
        window.addEventListener('scroll', function () {
            scrollTopBtn.classList.toggle('visible', window.pageYOffset > 400);
        });
        scrollTopBtn.addEventListener('click', function () {
            window.scrollTo({ top: 0, behavior: 'smooth' });
        });
    }

    // Basic in-page text search (highlights matches, jumps to the first one)
    const searchInput = document.getElementById('doc-search');
    if (searchInput) {
        let highlighted = [];

        function clearHighlights() {
            highlighted.forEach(function (el) {
                el.classList.remove('search-hit');
            });
            highlighted = [];
        }

        searchInput.addEventListener('input', function (e) {
            clearHighlights();
            const term = e.target.value.trim().toLowerCase();
            if (term.length < 2) return;

            const candidates = document.querySelectorAll('main h1, main h2, main h3, main p, main code');
            let scrolled = false;
            candidates.forEach(function (el) {
                if (el.textContent.toLowerCase().includes(term)) {
                    el.classList.add('search-hit');
                    highlighted.push(el);
                    if (!scrolled) {
                        el.scrollIntoView({ behavior: 'smooth', block: 'center' });
                        scrolled = true;
                    }
                }
            });
        });

        document.addEventListener('keydown', function (e) {
            if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
                e.preventDefault();
                searchInput.focus();
            }
            if (e.key === 'Escape' && document.activeElement === searchInput) {
                searchInput.blur();
                searchInput.value = '';
                clearHighlights();
            }
        });
    }

    if (typeof Prism !== 'undefined') Prism.highlightAll();
});
