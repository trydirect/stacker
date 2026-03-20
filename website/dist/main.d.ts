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
declare class ParticleSystem {
    private canvas;
    private ctx;
    private particles;
    private mouse;
    private animId;
    private readonly MAX_PARTICLES;
    private readonly CONNECTION_DIST;
    constructor(canvas: HTMLCanvasElement);
    private resize;
    private init;
    private createParticle;
    private bindEvents;
    private animate;
    destroy(): void;
}
declare class TypeWriter {
    private element;
    private commands;
    private outputs;
    private outputEl;
    private currentCmd;
    private charIndex;
    private typing;
    private speed;
    constructor(element: HTMLElement, outputEl: HTMLElement, commands: string[], outputs: string[][], speed?: number);
    private type;
    private showOutput;
    private nextCommand;
}
declare class ScrollAnimator {
    private observer;
    constructor();
}
declare class CounterAnimator {
    private observer;
    constructor();
    private animateCounter;
}
declare class StickyNav {
    private nav;
    private lastScrollY;
    constructor(nav: HTMLElement);
    private onScroll;
}
declare class MobileMenu {
    private toggle;
    private menu;
    private isOpen;
    constructor(toggle: HTMLElement, menu: HTMLElement);
    private toggleMenu;
    private close;
}
declare class CommandCardEffects {
    constructor();
}
declare function initSmoothScroll(): void;
declare function initCardGlow(): void;
