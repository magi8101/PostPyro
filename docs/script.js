// Navigation and Mobile Menu
document.addEventListener('DOMContentLoaded', function() {
    // Mobile hamburger menu
    const hamburger = document.querySelector('.hamburger');
    const navMenu = document.querySelector('.nav-menu');
    
    if (hamburger && navMenu) {
        hamburger.addEventListener('click', function() {
            navMenu.classList.toggle('active');
            hamburger.classList.toggle('active');
        });
    }
    
    // Smooth scrolling for navigation links
    const navLinks = document.querySelectorAll('.nav-link[href^="#"]');
    navLinks.forEach(link => {
        link.addEventListener('click', function(e) {
            e.preventDefault();
            const targetId = this.getAttribute('href');
            const targetSection = document.querySelector(targetId);
            
            if (targetSection) {
                const offsetTop = targetSection.offsetTop - 80; // Account for fixed navbar
                window.scrollTo({
                    top: offsetTop,
                    behavior: 'smooth'
                });
                
                // Close mobile menu if open
                navMenu.classList.remove('active');
                hamburger.classList.remove('active');
                
                // Update active nav link
                updateActiveNavLink(targetId);
            }
        });
    });
    
    // Update active navigation link on scroll
    function updateActiveNavLink(targetId) {
        navLinks.forEach(link => {
            link.classList.remove('active');
            if (link.getAttribute('href') === targetId) {
                link.classList.add('active');
            }
        });
    }
    
    // Intersection Observer for nav link highlighting
    const sections = document.querySelectorAll('section[id]');
    const observerOptions = {
        rootMargin: '-80px 0px -50% 0px',
        threshold: 0
    };
    
    const observer = new IntersectionObserver(function(entries) {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const id = '#' + entry.target.id;
                updateActiveNavLink(id);
            }
        });
    }, observerOptions);
    
    sections.forEach(section => {
        observer.observe(section);
    });
});

// Tab functionality for Quick Start section
function showTab(tabName) {
    // Hide all tab content
    const tabContents = document.querySelectorAll('.tab-content');
    tabContents.forEach(content => {
        content.classList.remove('active');
    });
    
    // Remove active class from all tab buttons
    const tabBtns = document.querySelectorAll('.tab-btn');
    tabBtns.forEach(btn => {
        btn.classList.remove('active');
    });
    
    // Show selected tab content
    const selectedTab = document.getElementById(`tab-${tabName}`);
    if (selectedTab) {
        selectedTab.classList.add('active');
    }
    
    // Add active class to clicked button
    event.target.classList.add('active');
    
    // Re-highlight code syntax
    if (typeof Prism !== 'undefined') {
        Prism.highlightAll();
    }
}

// Copy to clipboard functionality
function copyToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        // Modern approach
        navigator.clipboard.writeText(text).then(() => {
            showCopyFeedback(event.target);
        }).catch(err => {
            console.error('Failed to copy text: ', err);
            fallbackCopyTextToClipboard(text);
        });
    } else {
        // Fallback for older browsers
        fallbackCopyTextToClipboard(text);
    }
}

function fallbackCopyTextToClipboard(text) {
    const textArea = document.createElement('textarea');
    textArea.value = text;
    textArea.style.position = 'fixed';
    textArea.style.left = '-999999px';
    textArea.style.top = '-999999px';
    document.body.appendChild(textArea);
    textArea.focus();
    textArea.select();
    
    try {
        const successful = document.execCommand('copy');
        if (successful) {
            showCopyFeedback(event.target);
        }
    } catch (err) {
        console.error('Fallback: Oops, unable to copy', err);
    }
    
    document.body.removeChild(textArea);
}

function showCopyFeedback(button) {
    const originalHTML = button.innerHTML;
    button.innerHTML = `
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">
            <path d="M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z"/>
        </svg>
    `;
    button.style.color = '#06d6a0';
    
    setTimeout(() => {
        button.innerHTML = originalHTML;
        button.style.color = '';
    }, 2000);
}

// API Reference navigation
document.addEventListener('DOMContentLoaded', function() {
    const apiLinks = document.querySelectorAll('.api-link');
    const apiSections = document.querySelectorAll('.api-section');
    
    // Smooth scrolling for API navigation
    apiLinks.forEach(link => {
        link.addEventListener('click', function(e) {
            e.preventDefault();
            const targetId = this.getAttribute('href');
            const targetSection = document.querySelector(targetId);
            
            if (targetSection) {
                const offsetTop = targetSection.offsetTop - 100;
                window.scrollTo({
                    top: offsetTop,
                    behavior: 'smooth'
                });
                
                // Update active API link
                apiLinks.forEach(l => l.classList.remove('active'));
                this.classList.add('active');
            }
        });
    });
    
    // Intersection Observer for API sections
    const apiObserverOptions = {
        rootMargin: '-100px 0px -50% 0px',
        threshold: 0
    };
    
    const apiObserver = new IntersectionObserver(function(entries) {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                const id = '#' + entry.target.id;
                apiLinks.forEach(link => {
                    link.classList.remove('active');
                    if (link.getAttribute('href') === id) {
                        link.classList.add('active');
                    }
                });
            }
        });
    }, apiObserverOptions);
    
    apiSections.forEach(section => {
        apiObserver.observe(section);
    });
});

// Scroll to top functionality
function createScrollToTop() {
    const scrollToTopBtn = document.createElement('button');
    scrollToTopBtn.innerHTML = '↑';
    scrollToTopBtn.className = 'scroll-to-top';
    scrollToTopBtn.style.cssText = `
        position: fixed;
        bottom: 20px;
        right: 20px;
        width: 50px;
        height: 50px;
        border-radius: 50%;
        background: var(--primary);
        color: white;
        border: none;
        font-size: 20px;
        cursor: pointer;
        opacity: 0;
        visibility: hidden;
        transition: all 0.3s ease;
        z-index: 1000;
        box-shadow: var(--shadow-lg);
    `;
    
    document.body.appendChild(scrollToTopBtn);
    
    // Show/hide scroll to top button
    window.addEventListener('scroll', function() {
        if (window.pageYOffset > 300) {
            scrollToTopBtn.style.opacity = '1';
            scrollToTopBtn.style.visibility = 'visible';
        } else {
            scrollToTopBtn.style.opacity = '0';
            scrollToTopBtn.style.visibility = 'hidden';
        }
    });
    
    // Scroll to top on click
    scrollToTopBtn.addEventListener('click', function() {
        window.scrollTo({
            top: 0,
            behavior: 'smooth'
        });
    });
}

// Initialize scroll to top
document.addEventListener('DOMContentLoaded', createScrollToTop);

// Navbar background on scroll
document.addEventListener('DOMContentLoaded', function() {
    const navbar = document.querySelector('.navbar');
    
    window.addEventListener('scroll', function() {
        if (window.scrollY > 50) {
            navbar.style.background = 'rgba(15, 15, 35, 0.98)';
            navbar.style.backdropFilter = 'blur(15px)';
        } else {
            navbar.style.background = 'rgba(15, 15, 35, 0.95)';
            navbar.style.backdropFilter = 'blur(10px)';
        }
    });
});

// Animate elements on scroll
function animateOnScroll() {
    const elements = document.querySelectorAll('.feature-card, .example-card, .reason');
    
    const animationObserver = new IntersectionObserver(function(entries) {
        entries.forEach(entry => {
            if (entry.isIntersecting) {
                entry.target.style.opacity = '1';
                entry.target.style.transform = 'translateY(0)';
            }
        });
    }, {
        threshold: 0.1,
        rootMargin: '0px 0px -50px 0px'
    });
    
    elements.forEach(element => {
        element.style.opacity = '0';
        element.style.transform = 'translateY(20px)';
        element.style.transition = 'opacity 0.6s ease, transform 0.6s ease';
        animationObserver.observe(element);
    });
}

// Initialize animations
document.addEventListener('DOMContentLoaded', animateOnScroll);

// Search functionality (basic)
function initSearch() {
    const searchInput = document.createElement('input');
    searchInput.type = 'text';
    searchInput.placeholder = 'Search documentation...';
    searchInput.className = 'search-input';
    searchInput.style.cssText = `
        background: var(--bg-secondary);
        border: 1px solid var(--border-color);
        color: var(--text-primary);
        padding: var(--space-sm) var(--space-md);
        border-radius: var(--radius-md);
        font-family: inherit;
        font-size: 0.875rem;
        width: 200px;
        transition: var(--transition);
    `;
    
    // Add search to navigation (if space allows)
    const navMenu = document.querySelector('.nav-menu');
    if (navMenu && window.innerWidth > 1024) {
        const searchContainer = document.createElement('li');
        searchContainer.appendChild(searchInput);
        navMenu.insertBefore(searchContainer, navMenu.lastElementChild);
    }
    
    // Simple search functionality
    searchInput.addEventListener('input', function(e) {
        const searchTerm = e.target.value.toLowerCase().trim();
        
        if (searchTerm.length < 2) {
            clearSearchHighlights();
            return;
        }
        
        // Simple text search in content
        const contentElements = document.querySelectorAll('h1, h2, h3, h4, h5, h6, p, code');
        let found = false;
        
        contentElements.forEach(element => {
            const text = element.textContent.toLowerCase();
            if (text.includes(searchTerm)) {
                element.style.background = 'rgba(255, 107, 53, 0.2)';
                element.style.borderRadius = '4px';
                element.style.padding = '2px 4px';
                
                if (!found) {
                    element.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    found = true;
                }
            }
        });
    });
    
    function clearSearchHighlights() {
        const highlightedElements = document.querySelectorAll('[style*="rgba(255, 107, 53, 0.2)"]');
        highlightedElements.forEach(element => {
            element.style.background = '';
            element.style.borderRadius = '';
            element.style.padding = '';
        });
    }
    
    // Clear highlights when search is empty
    searchInput.addEventListener('blur', function() {
        if (!this.value.trim()) {
            clearSearchHighlights();
        }
    });
}

// Initialize search on larger screens
document.addEventListener('DOMContentLoaded', function() {
    if (window.innerWidth > 1024) {
        initSearch();
    }
});

// Performance monitoring (basic)
document.addEventListener('DOMContentLoaded', function() {
    // Log page load performance
    window.addEventListener('load', function() {
        if (typeof performance !== 'undefined' && performance.timing) {
            const loadTime = performance.timing.loadEventEnd - performance.timing.navigationStart;
            console.log(`PostPyro Docs loaded in ${loadTime}ms`);
        }
    });
});

// Theme toggle (future enhancement placeholder)
function initThemeToggle() {
    // Placeholder for future dark/light theme toggle
    // Currently using dark theme by default
}

// Keyboard shortcuts
document.addEventListener('keydown', function(e) {
    // Cmd/Ctrl + K to focus search
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault();
        const searchInput = document.querySelector('.search-input');
        if (searchInput) {
            searchInput.focus();
        }
    }
    
    // Escape to clear search highlights
    if (e.key === 'Escape') {
        const searchInput = document.querySelector('.search-input');
        if (searchInput && document.activeElement === searchInput) {
            searchInput.blur();
        }
    }
});

// Error handling for missing elements
function safeQuerySelector(selector, callback) {
    const element = document.querySelector(selector);
    if (element && typeof callback === 'function') {
        callback(element);
    }
}

// Initialize all functionality when DOM is ready
document.addEventListener('DOMContentLoaded', function() {
    console.log('PostPyro Documentation loaded successfully! 🔥');
    
    // Re-highlight all code blocks
    if (typeof Prism !== 'undefined') {
        Prism.highlightAll();
    }
});

// Service Worker registration (for offline support - future enhancement)
if ('serviceWorker' in navigator) {
    window.addEventListener('load', function() {
        // navigator.registerServiceWorker('/sw.js') - Future enhancement
    });
}

// Analytics placeholder (if needed in future)
function initAnalytics() {
    // Placeholder for analytics tracking
    // Track page views, section visits, etc.
}

// Export functions for testing (if needed)
if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        showTab,
        copyToClipboard,
        showCopyFeedback
    };
}