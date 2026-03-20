"use strict";
// ============================================================
// Stacker Website — TypeScript Interactions
// Particle system, typing effect, scroll animations, counters
// ============================================================
class ParticleSystem {
    constructor(canvas) {
        this.particles = [];
        this.mouse = { x: -1000, y: -1000 };
        this.animId = 0;
        this.MAX_PARTICLES = 80;
        this.CONNECTION_DIST = 120;
        this.animate = () => {
            this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
            this.particles.forEach((p, i) => {
                // Update position
                p.x += p.vx;
                p.y += p.vy;
                p.life++;
                // Mouse repulsion
                const dx = p.x - this.mouse.x;
                const dy = p.y - this.mouse.y;
                const dist = Math.sqrt(dx * dx + dy * dy);
                if (dist < 150) {
                    const force = (150 - dist) / 150;
                    p.vx += (dx / dist) * force * 0.02;
                    p.vy += (dy / dist) * force * 0.02;
                }
                // Damping
                p.vx *= 0.99;
                p.vy *= 0.99;
                // Wrap around
                if (p.x < 0)
                    p.x = this.canvas.width;
                if (p.x > this.canvas.width)
                    p.x = 0;
                if (p.y < 0)
                    p.y = this.canvas.height;
                if (p.y > this.canvas.height)
                    p.y = 0;
                // Fade lifecycle
                const lifeRatio = p.life / p.maxLife;
                const alpha = lifeRatio < 0.1
                    ? lifeRatio * 10 * p.opacity
                    : lifeRatio > 0.9
                        ? (1 - lifeRatio) * 10 * p.opacity
                        : p.opacity;
                // Draw particle
                this.ctx.beginPath();
                this.ctx.arc(p.x, p.y, p.size, 0, Math.PI * 2);
                this.ctx.fillStyle = `hsla(${p.hue}, 60%, 60%, ${alpha})`;
                this.ctx.fill();
                // Respawn
                if (p.life >= p.maxLife) {
                    this.particles[i] = this.createParticle();
                }
            });
            // Draw connections
            for (let i = 0; i < this.particles.length; i++) {
                for (let j = i + 1; j < this.particles.length; j++) {
                    const a = this.particles[i];
                    const b = this.particles[j];
                    const dx = a.x - b.x;
                    const dy = a.y - b.y;
                    const dist = Math.sqrt(dx * dx + dy * dy);
                    if (dist < this.CONNECTION_DIST) {
                        const opacity = (1 - dist / this.CONNECTION_DIST) * 0.15;
                        this.ctx.beginPath();
                        this.ctx.moveTo(a.x, a.y);
                        this.ctx.lineTo(b.x, b.y);
                        this.ctx.strokeStyle = `hsla(270, 50%, 60%, ${opacity})`;
                        this.ctx.lineWidth = 0.5;
                        this.ctx.stroke();
                    }
                }
            }
            this.animId = requestAnimationFrame(this.animate);
        };
        this.canvas = canvas;
        this.ctx = canvas.getContext('2d');
        this.resize();
        this.init();
        this.bindEvents();
        this.animate();
    }
    resize() {
        this.canvas.width = window.innerWidth;
        this.canvas.height = window.innerHeight;
    }
    init() {
        for (let i = 0; i < this.MAX_PARTICLES; i++) {
            this.particles.push(this.createParticle());
        }
    }
    createParticle() {
        const hueOptions = [270, 200, 340]; // purple, blue, pink
        return {
            x: Math.random() * this.canvas.width,
            y: Math.random() * this.canvas.height,
            vx: (Math.random() - 0.5) * 0.4,
            vy: (Math.random() - 0.5) * 0.4,
            size: Math.random() * 2 + 0.5,
            opacity: Math.random() * 0.5 + 0.1,
            hue: hueOptions[Math.floor(Math.random() * hueOptions.length)],
            life: 0,
            maxLife: Math.random() * 600 + 200,
        };
    }
    bindEvents() {
        window.addEventListener('resize', () => this.resize());
        window.addEventListener('mousemove', (e) => {
            this.mouse.x = e.clientX;
            this.mouse.y = e.clientY;
        });
    }
    destroy() {
        cancelAnimationFrame(this.animId);
    }
}
// --- Typing Effect ---
class TypeWriter {
    constructor(element, outputEl, commands, outputs, speed = 50) {
        this.currentCmd = 0;
        this.charIndex = 0;
        this.typing = true;
        this.element = element;
        this.outputEl = outputEl;
        this.commands = commands;
        this.outputs = outputs;
        this.speed = speed;
        this.type();
    }
    type() {
        const cmd = this.commands[this.currentCmd];
        if (this.charIndex < cmd.length) {
            this.element.textContent += cmd[this.charIndex];
            this.charIndex++;
            setTimeout(() => this.type(), this.speed + Math.random() * 40);
        }
        else {
            // Show output after command
            setTimeout(() => this.showOutput(), 400);
        }
    }
    showOutput() {
        const lines = this.outputs[this.currentCmd];
        let lineIndex = 0;
        const showLine = () => {
            if (lineIndex < lines.length) {
                const div = document.createElement('div');
                div.className = 'terminal__line terminal__line--output';
                const span = document.createElement('span');
                span.className = 'terminal__text';
                const line = lines[lineIndex];
                if (line.startsWith('✓')) {
                    span.classList.add('terminal__text--success');
                }
                else if (line.startsWith('▸') || line.startsWith('⟩')) {
                    span.classList.add('terminal__text--info');
                }
                span.textContent = line;
                div.appendChild(span);
                this.outputEl.appendChild(div);
                lineIndex++;
                setTimeout(showLine, 200);
            }
            else {
                // Move to next command
                setTimeout(() => this.nextCommand(), 2500);
            }
        };
        showLine();
    }
    nextCommand() {
        this.currentCmd = (this.currentCmd + 1) % this.commands.length;
        this.charIndex = 0;
        this.element.textContent = '';
        this.outputEl.innerHTML = '';
        this.type();
    }
}
// --- Scroll Animations ---
class ScrollAnimator {
    constructor() {
        this.observer = new IntersectionObserver((entries) => {
            entries.forEach((entry) => {
                if (entry.isIntersecting) {
                    const el = entry.target;
                    const delay = parseInt(el.dataset.delay || '0', 10);
                    setTimeout(() => {
                        el.classList.add('is-visible');
                    }, delay);
                    this.observer.unobserve(el);
                }
            });
        }, { threshold: 0.1, rootMargin: '0px 0px -50px 0px' });
        document.querySelectorAll('[data-animate]').forEach((el) => {
            this.observer.observe(el);
        });
    }
}
// --- Counter Animation ---
class CounterAnimator {
    constructor() {
        this.observer = new IntersectionObserver((entries) => {
            entries.forEach((entry) => {
                if (entry.isIntersecting) {
                    this.animateCounter(entry.target);
                    this.observer.unobserve(entry.target);
                }
            });
        }, { threshold: 0.5 });
        document.querySelectorAll('[data-count]').forEach((el) => {
            this.observer.observe(el);
        });
    }
    animateCounter(el) {
        const target = parseInt(el.dataset.count || '0', 10);
        const duration = 2000;
        const start = performance.now();
        const tick = (now) => {
            const elapsed = now - start;
            const progress = Math.min(elapsed / duration, 1);
            // Ease out cubic
            const eased = 1 - Math.pow(1 - progress, 3);
            const current = Math.round(eased * target);
            el.textContent = current.toString();
            if (progress < 1) {
                requestAnimationFrame(tick);
            }
            else {
                el.textContent = target.toString();
            }
        };
        requestAnimationFrame(tick);
    }
}
// --- Smooth Nav ---
class StickyNav {
    constructor(nav) {
        this.lastScrollY = 0;
        this.nav = nav;
        window.addEventListener('scroll', () => this.onScroll(), { passive: true });
        this.onScroll();
    }
    onScroll() {
        const scrollY = window.scrollY;
        if (scrollY > 80) {
            this.nav.classList.add('nav--scrolled');
        }
        else {
            this.nav.classList.remove('nav--scrolled');
        }
        this.lastScrollY = scrollY;
    }
}
// --- Mobile Menu ---
class MobileMenu {
    constructor(toggle, menu) {
        this.isOpen = false;
        this.toggle = toggle;
        this.menu = menu;
        this.toggle.addEventListener('click', () => this.toggleMenu());
        // Close on link click
        menu.querySelectorAll('.nav__link').forEach((link) => {
            link.addEventListener('click', () => this.close());
        });
    }
    toggleMenu() {
        this.isOpen = !this.isOpen;
        this.menu.classList.toggle('is-open', this.isOpen);
    }
    close() {
        this.isOpen = false;
        this.menu.classList.remove('is-open');
    }
}
// --- Command Card Hover Effects ---
class CommandCardEffects {
    constructor() {
        document.querySelectorAll('.command-card').forEach((card) => {
            card.addEventListener('mouseenter', (e) => {
                const el = e.currentTarget;
                el.style.setProperty('--glow-opacity', '1');
            });
            card.addEventListener('mouseleave', (e) => {
                const el = e.currentTarget;
                el.style.setProperty('--glow-opacity', '0');
            });
        });
    }
}
// --- Smooth Scroll for Anchor Links ---
function initSmoothScroll() {
    document.querySelectorAll('a[href^="#"]').forEach((anchor) => {
        anchor.addEventListener('click', (e) => {
            e.preventDefault();
            const href = anchor.getAttribute('href');
            if (!href || href === '#')
                return;
            const target = document.querySelector(href);
            if (target) {
                const navHeight = document.querySelector('.nav')?.getBoundingClientRect().height || 0;
                const top = target.getBoundingClientRect().top + window.scrollY - navHeight - 20;
                window.scrollTo({ top, behavior: 'smooth' });
            }
        });
    });
}
// --- Mouse Glow Effect on Feature Cards ---
function initCardGlow() {
    document.querySelectorAll('.feature-card, .provider-card, .cloud-logo').forEach((card) => {
        card.addEventListener('mousemove', (e) => {
            const mouseEvent = e;
            const rect = card.getBoundingClientRect();
            const x = mouseEvent.clientX - rect.left;
            const y = mouseEvent.clientY - rect.top;
            card.style.setProperty('--mouse-x', `${x}px`);
            card.style.setProperty('--mouse-y', `${y}px`);
        });
    });
}
// ============================================================
// Initialize Everything
// ============================================================
document.addEventListener('DOMContentLoaded', () => {
    // Particles
    const canvas = document.getElementById('particles');
    if (canvas) {
        new ParticleSystem(canvas);
    }
    // Typing effect in hero terminal
    const heroCommand = document.getElementById('heroCommand');
    const heroOutput = document.getElementById('heroOutput');
    if (heroCommand && heroOutput) {
        new TypeWriter(heroCommand, heroOutput, [
            'stacker init --with-ai',
            'stacker deploy --cloud hetzner',
            'stacker status',
            'stacker ai ask "optimize my config"',
        ], [
            [
                '▸ Scanning project structure...',
                '✓ Detected: Python + FastAPI + PostgreSQL + Redis',
                '✓ AI generating configuration via Ollama (deepseek-r1)',
                '✓ stacker.yml created with 6 services',
            ],
            [
                '▸ Provisioning infrastructure on Hetzner Cloud...',
                '▸ Running Terraform plan (3 resources)...',
                '▸ Configuring with Ansible...',
                '✓ Stack deployed! https://my-app.example.com',
            ],
            [
                '⟩ Stack: my-project (running)',
                '⟩ Services: 6/6 healthy',
                '⟩ Uptime: 14d 6h 32m',
                '✓ All systems operational',
            ],
            [
                '▸ Analyzing your stacker.yml...',
                '✓ Suggestion: Add health checks to redis service',
                '✓ Suggestion: Set memory limits for postgres (512MB → 1GB)',
                '✓ Config optimized. Run "stacker deploy" to apply.',
            ],
        ], 45);
    }
    // Scroll animations
    new ScrollAnimator();
    // Counter animations
    new CounterAnimator();
    // Sticky nav
    const nav = document.getElementById('nav');
    if (nav) {
        new StickyNav(nav);
    }
    // Mobile menu
    const mobileToggle = document.getElementById('mobileToggle');
    const navLinks = document.getElementById('navLinks');
    if (mobileToggle && navLinks) {
        new MobileMenu(mobileToggle, navLinks);
    }
    // Card effects
    new CommandCardEffects();
    // Smooth scroll
    initSmoothScroll();
    // Card glow
    initCardGlow();
});
//# sourceMappingURL=main.js.map