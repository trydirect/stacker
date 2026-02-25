// ============================================================
// Stacker Website — TypeScript Interactions
// Particle system, typing effect, scroll animations, counters
// ============================================================

// --- Particle System ---
interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  size: number;
  opacity: number;
  hue: number;
  life: number;
  maxLife: number;
}

class ParticleSystem {
  private canvas: HTMLCanvasElement;
  private ctx: CanvasRenderingContext2D;
  private particles: Particle[] = [];
  private mouse = { x: -1000, y: -1000 };
  private animId: number = 0;
  private readonly MAX_PARTICLES = 80;
  private readonly CONNECTION_DIST = 120;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d')!;
    this.resize();
    this.init();
    this.bindEvents();
    this.animate();
  }

  private resize(): void {
    this.canvas.width = window.innerWidth;
    this.canvas.height = window.innerHeight;
  }

  private init(): void {
    for (let i = 0; i < this.MAX_PARTICLES; i++) {
      this.particles.push(this.createParticle());
    }
  }

  private createParticle(): Particle {
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

  private bindEvents(): void {
    window.addEventListener('resize', () => this.resize());
    window.addEventListener('mousemove', (e: MouseEvent) => {
      this.mouse.x = e.clientX;
      this.mouse.y = e.clientY;
    });
  }

  private animate = (): void => {
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
      if (p.x < 0) p.x = this.canvas.width;
      if (p.x > this.canvas.width) p.x = 0;
      if (p.y < 0) p.y = this.canvas.height;
      if (p.y > this.canvas.height) p.y = 0;

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

  destroy(): void {
    cancelAnimationFrame(this.animId);
  }
}

// --- Typing Effect ---
class TypeWriter {
  private element: HTMLElement;
  private commands: string[];
  private outputs: string[][];
  private outputEl: HTMLElement;
  private currentCmd = 0;
  private charIndex = 0;
  private typing = true;
  private speed: number;

  constructor(
    element: HTMLElement,
    outputEl: HTMLElement,
    commands: string[],
    outputs: string[][],
    speed = 50
  ) {
    this.element = element;
    this.outputEl = outputEl;
    this.commands = commands;
    this.outputs = outputs;
    this.speed = speed;
    this.type();
  }

  private type(): void {
    const cmd = this.commands[this.currentCmd];
    if (this.charIndex < cmd.length) {
      this.element.textContent += cmd[this.charIndex];
      this.charIndex++;
      setTimeout(() => this.type(), this.speed + Math.random() * 40);
    } else {
      // Show output after command
      setTimeout(() => this.showOutput(), 400);
    }
  }

  private showOutput(): void {
    const lines = this.outputs[this.currentCmd];
    let lineIndex = 0;

    const showLine = (): void => {
      if (lineIndex < lines.length) {
        const div = document.createElement('div');
        div.className = 'terminal__line terminal__line--output';
        const span = document.createElement('span');
        span.className = 'terminal__text';
        const line = lines[lineIndex];
        if (line.startsWith('✓')) {
          span.classList.add('terminal__text--success');
        } else if (line.startsWith('▸') || line.startsWith('⟩')) {
          span.classList.add('terminal__text--info');
        }
        span.textContent = line;
        div.appendChild(span);
        this.outputEl.appendChild(div);
        lineIndex++;
        setTimeout(showLine, 200);
      } else {
        // Move to next command
        setTimeout(() => this.nextCommand(), 2500);
      }
    };

    showLine();
  }

  private nextCommand(): void {
    this.currentCmd = (this.currentCmd + 1) % this.commands.length;
    this.charIndex = 0;
    this.element.textContent = '';
    this.outputEl.innerHTML = '';
    this.type();
  }
}

// --- Scroll Animations ---
class ScrollAnimator {
  private observer: IntersectionObserver;

  constructor() {
    this.observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            const el = entry.target as HTMLElement;
            const delay = parseInt(el.dataset.delay || '0', 10);
            setTimeout(() => {
              el.classList.add('is-visible');
            }, delay);
            this.observer.unobserve(el);
          }
        });
      },
      { threshold: 0.1, rootMargin: '0px 0px -50px 0px' }
    );

    document.querySelectorAll('[data-animate]').forEach((el) => {
      this.observer.observe(el);
    });
  }
}

// --- Counter Animation ---
class CounterAnimator {
  private observer: IntersectionObserver;

  constructor() {
    this.observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            this.animateCounter(entry.target as HTMLElement);
            this.observer.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.5 }
    );

    document.querySelectorAll('[data-count]').forEach((el) => {
      this.observer.observe(el);
    });
  }

  private animateCounter(el: HTMLElement): void {
    const target = parseInt(el.dataset.count || '0', 10);
    const duration = 2000;
    const start = performance.now();

    const tick = (now: number): void => {
      const elapsed = now - start;
      const progress = Math.min(elapsed / duration, 1);
      // Ease out cubic
      const eased = 1 - Math.pow(1 - progress, 3);
      const current = Math.round(eased * target);
      el.textContent = current.toString();

      if (progress < 1) {
        requestAnimationFrame(tick);
      } else {
        el.textContent = target.toString();
      }
    };

    requestAnimationFrame(tick);
  }
}

// --- Smooth Nav ---
class StickyNav {
  private nav: HTMLElement;
  private lastScrollY = 0;

  constructor(nav: HTMLElement) {
    this.nav = nav;
    window.addEventListener('scroll', () => this.onScroll(), { passive: true });
    this.onScroll();
  }

  private onScroll(): void {
    const scrollY = window.scrollY;
    if (scrollY > 80) {
      this.nav.classList.add('nav--scrolled');
    } else {
      this.nav.classList.remove('nav--scrolled');
    }
    this.lastScrollY = scrollY;
  }
}

// --- Mobile Menu ---
class MobileMenu {
  private toggle: HTMLElement;
  private menu: HTMLElement;
  private isOpen = false;

  constructor(toggle: HTMLElement, menu: HTMLElement) {
    this.toggle = toggle;
    this.menu = menu;
    this.toggle.addEventListener('click', () => this.toggleMenu());
    // Close on link click
    menu.querySelectorAll('.nav__link').forEach((link) => {
      link.addEventListener('click', () => this.close());
    });
  }

  private toggleMenu(): void {
    this.isOpen = !this.isOpen;
    this.menu.classList.toggle('is-open', this.isOpen);
  }

  private close(): void {
    this.isOpen = false;
    this.menu.classList.remove('is-open');
  }
}

// --- Command Card Hover Effects ---
class CommandCardEffects {
  constructor() {
    document.querySelectorAll('.command-card').forEach((card) => {
      card.addEventListener('mouseenter', (e) => {
        const el = e.currentTarget as HTMLElement;
        el.style.setProperty('--glow-opacity', '1');
      });
      card.addEventListener('mouseleave', (e) => {
        const el = e.currentTarget as HTMLElement;
        el.style.setProperty('--glow-opacity', '0');
      });
    });
  }
}

// --- Smooth Scroll for Anchor Links ---
function initSmoothScroll(): void {
  document.querySelectorAll('a[href^="#"]').forEach((anchor) => {
    anchor.addEventListener('click', (e: Event) => {
      e.preventDefault();
      const href = (anchor as HTMLAnchorElement).getAttribute('href');
      if (!href || href === '#') return;
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
function initCardGlow(): void {
  document.querySelectorAll('.feature-card, .provider-card, .cloud-logo').forEach((card) => {
    card.addEventListener('mousemove', (e: Event) => {
      const mouseEvent = e as MouseEvent;
      const rect = (card as HTMLElement).getBoundingClientRect();
      const x = mouseEvent.clientX - rect.left;
      const y = mouseEvent.clientY - rect.top;
      (card as HTMLElement).style.setProperty('--mouse-x', `${x}px`);
      (card as HTMLElement).style.setProperty('--mouse-y', `${y}px`);
    });
  });
}

// ============================================================
// Initialize Everything
// ============================================================
document.addEventListener('DOMContentLoaded', () => {
  // Particles
  const canvas = document.getElementById('particles') as HTMLCanvasElement;
  if (canvas) {
    new ParticleSystem(canvas);
  }

  // Typing effect in hero terminal
  const heroCommand = document.getElementById('heroCommand');
  const heroOutput = document.getElementById('heroOutput');
  if (heroCommand && heroOutput) {
    new TypeWriter(
      heroCommand,
      heroOutput,
      [
        'stacker init --with-ai',
        'stacker deploy --cloud hetzner',
        'stacker status',
        'stacker ai ask "optimize my config"',
      ],
      [
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
      ],
      45
    );
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
