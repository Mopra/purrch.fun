<script lang="ts">
  import { onMount } from "svelte";
  import {
    renderScene,
    IDLE_POSE,
    SCENE_W,
    SCENE_H,
    CAT_X,
    CAT_Y,
    type CatPose,
    type Particle,
  } from "./lib/render.ts";
  import {
    HEART,
    HEART_SMALL,
    ZZ_BIG,
    ZZ_SMALL,
    THINK_DOT,
    EAR_WAVE,
    EAR_WAVE_SMALL,
    SPARK,
    OOPS,
    PUFF,
    PUFF_SMALL,
    MOUSE,
    PILE,
    YARN_FRAMES,
    YARN_H,
    YARN_W,
  } from "./lib/catArt.ts";
  import { coatById, type Coat } from "./lib/coats.ts";
  import * as perch from "./lib/perch.ts";
  import { recall, remember } from "./lib/memory.ts";

  let {
    ondblclick,
    panelOpen = false,
    coat = coatById(null),
    gifts = 0,
  }: {
    ondblclick?: () => void;
    panelOpen?: boolean;
    coat?: Coat;
    /**
     * Unread gifts waiting to be looked at. Drawn as a pile of mice by the
     * cat's paws — the layer that works with the panel shut and nobody
     * reading anything: you glance over and the cat has been out.
     */
    gifts?: number;
  } = $props();

  const SCALE = 5;
  const TICK_MS = 100;

  let canvas: HTMLCanvasElement;

  // "think" / "work" persist until the turn ends; "done" / "oops" are brief
  // reactions that fall back to idle on their own.
  type Mood =
    | "idle"
    | "groom"
    | "sleep"
    | "pet"
    | "play"
    | "think"
    | "work"
    | "done"
    | "oops"
    | "curious"
    | "listen";

  /** Moods driven by the agent bridge, which suppress idle behaviours. */
  const BUSY: Mood[] = ["think", "work"];

  interface Sprite extends Particle {
    vx: number;
    vy: number;
    life: number;
  }

  // --- behavior state (plain vars; the tick loop drives the canvas) ---
  let mood: Mood = "idle";
  let tick = 0;
  let tailPhase = 0;
  let blinkAt = 30;
  let blinkLeft = 0;
  let gazeLeft = 0;
  let gaze = 0;
  let groomAt = rand(250, 600);
  let groomLeft = 0;
  let petLeft = 0;
  let reactLeft = 0; // countdown for the brief "done" / "oops" reactions
  let pawPhase = 0; // front paw cycle while working
  let watchX = 0; // -1 | 0 | 1 — which way the eyes follow a dangled file
  let moodBefore: Mood = "idle"; // what to go back to once the file is gone
  let listenLeft = 0; // ticks the ears stay up waiting for the rest of a sentence
  let sleepAfter = rand(1500, 2400); // 2.5–4 min of no interaction
  let lastTouch = 0;
  let sprites: Sprite[] = [];

  // --- memory ---
  // The cat picks up where it left off: same spot on the taskbar (the window
  // is already standing there — Rust puts it back before the window is shown),
  // same state, same tally of everything you've done together.
  //
  // Time keeps passing while Purrch is closed, so a cat left alone for longer
  // than it would have stayed awake is found asleep, exactly as it would have
  // been had the app never been shut.
  const DOZE_MS = 5 * 60 * 1000;
  /** Nothing is written down until we know what there is to remember. */
  let loaded = false;
  let pets = 0;
  let naps = 0;
  let plays = 0;
  /** Last values committed, so a tick with nothing new writes nothing. */
  let kept = { x: NaN, y: NaN, asleep: false, pets: 0, naps: 0, plays: 0 };

  /** Writes down anything that changed. The commit itself is debounced. */
  function keep() {
    if (!loaded) return;
    const patch: Partial<typeof kept> = {};
    // Only while the panel is shut is the window the cat: with it open the
    // window is wider and taller and grows up and to the left, so its corner
    // is nowhere near the cat's feet. Writing that down would stand the cat a
    // panel's width further left on every launch. Closing the panel puts the
    // window back around the cat, and the next tick records it properly.
    const x = Math.round(winX);
    const y = Math.round(winY);
    if (!panelOpen && (x !== kept.x || y !== kept.y)) {
      patch.x = kept.x = x;
      patch.y = kept.y = y;
    }
    const asleep = mood === "sleep";
    if (asleep !== kept.asleep) {
      patch.asleep = kept.asleep = asleep;
      // Counted here rather than at every place the cat can nod off, so a nap
      // is a nap however it started.
      if (asleep) patch.naps = kept.naps = ++naps;
    }
    if (pets !== kept.pets) patch.pets = kept.pets = pets;
    if (plays !== kept.plays) patch.plays = kept.plays = plays;
    if (Object.keys(patch).length > 0) remember(patch);
  }

  async function restore() {
    const memory = await recall();
    if (memory) {
      pets = memory.pets;
      naps = memory.naps;
      plays = memory.plays;
      kept = {
        ...kept,
        asleep: memory.asleep,
        pets,
        naps,
        plays,
      };
      // `mood` guard: the read is async, and anything that has already woken
      // the cat in those few frames outranks how it was left.
      if (
        mood === "idle" &&
        (memory.asleep || Date.now() - memory.lastSeen > DOZE_MS)
      ) {
        mood = "sleep";
      }
    }
    loaded = true;
  }

  // --- yarn ball ---
  //
  // The only thing that stops the ball is the cat's own paws, on the right.
  // There is deliberately no wall on the left: the window edge isn't a wall,
  // it's just where we stop being able to draw, so a ball that bounces off it
  // reads as a ball in a box. Instead it rolls out of frame — under a window,
  // behind the desk, wherever — and comes back a moment later, which is what a
  // batted yarn ball does anyway.
  const YARN_FLOOR = SCENE_H - YARN_H; // ball's y when it rests on the floor
  const YARN_OFF = -YARN_W - 1; // fully out of frame, off to the left
  const YARN_STOP = CAT_X; // the cat's paws block it going further right
  const YARN_REACH = CAT_X - 2.5; // close enough to take a swipe at it
  const GRAVITY = 0.3;
  const ROLL_FRICTION = 0.94;

  let ballOn = false;
  let ballX = 0;
  let ballY = YARN_FLOOR;
  let ballVX = 0;
  let ballVY = 0;
  let ballSpin = 0; // accumulated roll, picks which wound-thread frame to draw
  let ballStill = 0; // ticks the ball has sat motionless out of reach
  let ballAway = 0; // ticks out of frame before it rolls back in
  let playLeft = 0; // ticks left in the session before the cat loses interest
  let swatLeft = 0; // ticks the paw stays extended after a swipe
  let playAt = rand(600, 1200);

  // --- gravity ---
  // The cat lives on the taskbar. Horizontal moves shift the window along it;
  // the only vertical window movement is falling back down after being picked
  // up. Jumps are drawn inside the scene instead (see `lift`), which keeps the
  // window welded to the ground line so the yarn — and the taskbar it rests on
  // — don't slide around underneath the cat mid-pounce.
  // Window movement runs on animation frames rather than the 10fps art tick,
  // otherwise a fall would visibly teleport down the screen in chunks.
  //
  // `winX` / `winY` are physical pixels on the virtual desktop (see perch.ts),
  // so the distances below are multiplied by the current monitor's scale to
  // keep the cat moving at the same apparent speed on every screen.
  const WALK_SPEED = 34; // logical px per second while pacing
  const PACE_RANGE = 110; // how far either side of home it will wander
  const FALL_G = 2400; // logical px per second^2 once dropped
  const FALL_MAX = 1600; // terminal velocity, so a long drop isn't absurd
  const JUMP_V = 2.5; // scene px per art tick — the pounce
  const JUMP_G = 0.45; // ~30 logical px up, back down inside a second
  const DRAG_SLOP = 4; // pointer travel before a click becomes a drag

  // --- being carried ---
  // Picked up, the cat hangs from the scruff and everything below the neck
  // swings on the end of it: hauling the pointer one way leaves the body
  // trailing the other, and stopping lets it swing itself still. All in scene
  // pixels of sideways bend at the back paws.
  const SWING_K = 0.3; // spring pulling the body back under the scruff
  const SWING_DAMP = 0.78; // energy left after each tick
  const SWING_THROW = 0.05; // bend per CSS pixel of pointer travel
  const SWING_TOP = 60; // travel past this in one tick doesn't throw it further
  const SWING_MAX = 4;
  const LIMP_TICKS = 2; // beats after release before it twists itself upright

  /** Every monitor, refreshed on the same beat as the ground line. */
  let world: perch.Perch | null = null;
  /** Limits for the monitor the cat is on right now. */
  let limits: perch.Ground | null = null;
  let winX = 0;
  let winY = 0;
  let fallVY = 0;
  let falling = false;
  let lift = 0; // scene px off the ground, mid-jump
  let liftV = 0;
  let squash = 0; // ticks left of the fold after a drop from height
  let dip = 0; // ...or of the knee-bend after a pounce, which is far less
  let rebound = 0; // ticks left of the pop back up onto its feet
  let limp = 0; // dropped, still dangling before it rights itself
  let swing = 0; // where the dangling half is, relative to the scruff
  let swingV = 0;
  let carriedDX = 0; // pointer travel since the last art tick, in CSS px
  let walkDir: 1 | -1 = -1;
  let homeX = 0; // centre of the pacing range

  function clamp(v: number, lo: number, hi: number): number {
    return v < lo ? lo : v > hi ? hi : v;
  }

  /**
   * Works out which monitor the cat is standing on now and pins it to that
   * one's ground. Returns true if it had to be moved to get there.
   *
   * A taller taskbar on the monitor it just stepped onto pushes it straight up
   * — that's a step, not a fall. A shorter one leaves it above the new floor,
   * and `stepGround` drops it the rest of the way.
   */
  function reground(): boolean {
    if (!world) return false;
    const from = limits?.screen ?? world.current;
    limits = perch.ground(world, perch.under(world, winX, winY, from));
    const x = clamp(winX, limits.minX, limits.maxX);
    const y = Math.min(winY, limits.floor);
    if (x === winX && y === winY) return false;
    winX = x;
    winY = y;
    return true;
  }

  /**
   * Re-reads the monitors — after a resize, if a taskbar moves, or when a
   * screen is plugged in or unplugged. Only adopts the window's reported
   * position when the cat is standing still; mid-move our own state is the
   * newer of the two.
   */
  export async function resync(adopt = true) {
    const p = await perch.read();
    if (!p) return;
    world = p;
    if (adopt) {
      limits = perch.ground(p, p.current);
      winX = clamp(p.x, limits.minX, limits.maxX);
      winY = Math.min(p.y, limits.floor);
      homeX = winX;
      return;
    }
    if (reground()) place();
  }

  /** Pacing along the taskbar while the agent works. */
  function walking(): boolean {
    return mood === "work" && !panelOpen && !falling && !dragging && !!world;
  }

  /** True while something other than the user is moving the cat. */
  function moving(): boolean {
    return falling || dragging || walking();
  }

  // One window move per frame at most, however often the physics ask.
  let moveQueued = false;
  function place() {
    if (moveQueued) return;
    moveQueued = true;
    requestAnimationFrame(() => {
      moveQueued = false;
      perch.moveTo(winX, winY);
    });
  }

  /** Sends the cat up. Ignored if it isn't standing on something. */
  function jump() {
    if (lift === 0 && liftV === 0) liftV = JUMP_V;
  }

  /** Off a drag, the cat is briefly still limp; scruffed, it is dangling. */
  function carried(): boolean {
    return dragging || limp > 0;
  }

  /**
   * Feet down. A drop from height folds the cat into the floor and knocks the
   * dust out from under it; a pounce just bends the knees on the way back up.
   */
  function touchdown(hard: boolean) {
    if (hard) {
      squash = 3;
      rebound = 2;
      puff();
    } else {
      dip = 3;
    }
    limp = 0;
    swing = 0;
    swingV = 0;
  }

  /** The in-canvas hop, on the art tick so it stays in step with the pixels. */
  function stepAir() {
    // The fold and the bounce back out of it run back to back, never together.
    if (squash > 0) squash--;
    else if (rebound > 0) rebound--;
    if (dip > 0) dip--;
    if (limp > 0) limp--;
    if (lift === 0 && liftV === 0) return;
    liftV -= JUMP_G;
    lift += liftV;
    if (lift <= 0) {
      lift = 0;
      liftV = 0;
      touchdown(false);
    }
  }

  /**
   * A pendulum for the scruffed cat's body: the pointer throws it, a spring
   * pulls it back under the hand, and damping means it settles after a couple
   * of passes instead of either snapping straight or wobbling forever.
   */
  function stepSwing() {
    if (!carried()) {
      // back on its own four feet — whatever swing is left goes quickly
      swing = Math.abs(swing) < 0.3 ? 0 : swing * 0.5;
      swingV = 0;
      carriedDX = 0;
      return;
    }
    swingV += -swing * SWING_K - clamp(carriedDX, -SWING_TOP, SWING_TOP) * SWING_THROW;
    swingV *= SWING_DAMP;
    swing = clamp(swing + swingV, -SWING_MAX, SWING_MAX);
    carriedDX = 0;
  }

  /** Window movement, per animation frame. `dt` is in seconds. */
  function stepGround(dt: number) {
    if (!limits || dragging) return;
    const scale = limits.scale;

    // Falling: the cat was picked up and let go, so it drops back down to the
    // taskbar. This is the only time the window itself moves vertically.
    if (winY < limits.floor - 0.5) {
      falling = true;
      fallVY = Math.min(fallVY + FALL_G * scale * dt, FALL_MAX * scale);
      winY = Math.min(winY + fallVY * dt, limits.floor);
      if (winY >= limits.floor) {
        fallVY = 0;
        falling = false;
        touchdown(true);
        homeX = winX; // it paces around wherever it was put down
      }
      void perch.moveTo(winX, winY);
      return; // no walking while it's still in the air
    }

    // Pacing, only while the agent is actually working. With the chat panel
    // open the cat is attached to a big window, so it stays put.
    if (!walking()) return;
    const reach = PACE_RANGE * scale;
    const min = Math.max(limits.minX, homeX - reach);
    const max = Math.min(limits.maxX, homeX + reach);
    if (max - min < 1) return; // nowhere to go
    winX += WALK_SPEED * scale * dt * walkDir;
    if (winX <= min) {
      winX = min;
      walkDir = 1;
    } else if (winX >= max) {
      winX = max;
      walkDir = -1;
    }
    // A stride can carry it over the seam onto the next monitor, where the
    // taskbar — and the ground with it — is somewhere else entirely.
    reground();
    void perch.moveTo(winX, winY);
  }

  function rand(min: number, max: number): number {
    return Math.floor(min + Math.random() * (max - min));
  }

  function touch() {
    lastTouch = tick;
    if (mood === "sleep") {
      mood = "idle";
      sprites = [];
    }
  }

  // Particles are placed relative to the cat rather than to the scene: the
  // scene is only as big as it is to give them somewhere to drift, so what
  // matters is where they sit against the cat's head, not the window.

  /** A puff of hearts around the head — cuddles, and a finished turn. */
  function hearts() {
    for (let i = 0; i < rand(3, 6); i++) {
      sprites.push({
        grid: Math.random() < 0.5 ? HEART : HEART_SMALL,
        x: rand(CAT_X - 4, CAT_X + 24),
        y: rand(CAT_Y - 8, CAT_Y - 1),
        vx: Math.random() < 0.5 ? -1 : 1,
        vy: -1,
        life: rand(14, 24),
      });
    }
  }

  /** Dust knocked sideways out from under the paws on a hard landing. */
  function puff() {
    for (const side of [-1, 1] as const) {
      for (let i = 0; i < 2; i++) {
        sprites.push({
          grid: i === 0 ? PUFF : PUFF_SMALL,
          x: CAT_X + (side < 0 ? rand(-4, 1) : rand(19, 24)),
          y: SCENE_H - rand(5, 8),
          vx: side,
          vy: -1,
          life: rand(5, 9),
        });
      }
    }
  }

  export function pet() {
    touch();
    mood = "pet";
    petLeft = 14;
    pets++;
    if (ballOn) {
      // the game pauses for a cuddle — let the ball settle rather than
      // leaving it hanging mid-hop until the cat gets back to it
      ballY = YARN_FLOOR;
      ballVY = 0;
      ballVX = 0;
      swatLeft = 0;
    }
    hearts();
  }

  export function nap() {
    stopPlay();
    mood = "sleep";
    sprites = [];
  }

  /**
   * A file is being held over the cat. Nothing lights up around it — the cat
   * *is* the drop target, so the invitation has to come off the animal: it
   * looks up at whatever you're dangling, tail flicking, a paw already up.
   *
   * `x` is where the file is, in physical pixels from the window's left edge,
   * so the eyes can follow it as it moves.
   */
  export function curious(over: boolean, x?: number) {
    if (!over) {
      if (mood === "curious") mood = moodBefore;
      return;
    }
    if (x !== undefined && canvas) {
      // Physical px against the canvas's CSS-pixel box — the same conversion
      // the drag handling does, for the same reason.
      const box = canvas.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const off = (x / dpr - (box.left + box.width / 2)) / (box.width / 2);
      watchX = off < -0.2 ? -1 : off > 0.2 ? 1 : 0;
    }
    if (mood === "curious") return;
    touch();
    stopPlay();
    // Only a turn in progress is worth going back to afterwards; a nap or a
    // game of yarn is over the moment there's a file on offer.
    moodBefore = BUSY.includes(mood) ? mood : "idle";
    mood = "curious";
    sprites = [];
  }

  // --- being spoken to ---

  /**
   * How long the ears stay up on nothing but a name.
   *
   * Long enough to cover a sentence with a pause in the middle of it, short
   * enough that a cat which was talked over — its name caught in somebody
   * else's conversation — settles back down while you're still watching, so
   * the mistake reads as a cat mishearing rather than as a cat stuck.
   */
  const LISTEN_TICKS = 45;

  /**
   * The cat's name has just been said, and the sentence is still coming.
   *
   * Deliberately refuses while there's a turn in progress or a file overhead:
   * both of those are the cat already doing something for you, and neither
   * pose survives being interrupted halfway to say "I'm listening".
   */
  export function listen() {
    if (BUSY.includes(mood) || mood === "curious") return;
    touch();
    stopPlay();
    mood = "listen";
    listenLeft = LISTEN_TICKS;
    sprites = [];
  }

  /** Called, then nothing usable came out of it. The ears drop. */
  export function unheard() {
    if (mood === "listen") {
      mood = "idle";
      sprites = [];
    }
  }

  // --- yarn ball ---

  /**
   * Roll the ball in from out of frame, hard enough that friction alone
   * carries it all the way back to the cat's paws.
   */
  function rollIn() {
    ballAway = 0;
    ballX = YARN_OFF;
    ballY = YARN_FLOOR;
    ballVX = 1.4 + Math.random() * 0.4;
    ballVY = 0;
    ballStill = 0;
  }

  /** Roll the ball in from off-screen left and start a play session. */
  function startPlay() {
    mood = "play";
    sprites = [];
    plays++;
    // scheduled up front, like grooming, so an interrupted session still
    // pushes the next one out instead of retriggering the moment it idles
    playAt = tick + rand(700, 1500);
    playLeft = rand(160, 280); // 16–28s of batting it about
    swatLeft = 0;
    ballOn = true;
    ballSpin = 0;
    rollIn();
  }

  /** Put the yarn away — used when something more important comes up. */
  function stopPlay() {
    ballOn = false;
    playLeft = 0;
    swatLeft = 0;
    ballAway = 0;
  }

  /** The session is over: pack up and don't groom the instant it's gone. */
  function endPlay() {
    stopPlay();
    mood = "idle";
    groomAt = tick + rand(200, 500);
  }

  export function play() {
    touch();
    startPlay();
  }

  function stepPlay() {
    if (playLeft > 0) playLeft--;
    // Once the session is up the cat gives the ball one last shove and lets it
    // roll off screen, rather than having it blink out of existence.
    const leaving = playLeft === 0;

    // Out of frame: the cat stays crouched over the spot it vanished into
    // until it comes rolling back.
    if (ballAway > 0) {
      if (leaving) endPlay();
      else if (--ballAway === 0) rollIn();
      return;
    }

    const grounded = ballY >= YARN_FLOOR - 0.5;

    if (swatLeft > 0) {
      swatLeft--;
    } else if (!leaving && grounded) {
      if (ballX >= YARN_REACH && ballVX > -0.1) {
        // rolled into paw range — bat it back across the floor, with a hop
        swatLeft = 5;
        ballVX = -(1 + Math.random() * 0.5);
        ballVY = -1.4;
        ballStill = 0;
        if (Math.random() < 0.45) jump(); // sometimes it commits and pounces
      } else if (Math.abs(ballVX) < 0.08 && ++ballStill > 12) {
        // stopped out of reach — lean over and hook it back in
        swatLeft = 5;
        ballVX = 0.5 + Math.random() * 0.3;
        ballStill = 0;
      }
    } else {
      ballStill = 0;
    }

    if (leaving && ballVX > -1.5) ballVX = -1.5;

    ballX += ballVX;
    ballVY += GRAVITY;
    ballY += ballVY;
    ballSpin += ballVX * 0.6;

    if (ballY >= YARN_FLOOR) {
      ballY = YARN_FLOOR;
      ballVY = ballVY > 0.7 ? -ballVY * 0.4 : 0;
      if (!leaving) ballVX *= ROLL_FRICTION;
    }

    // The paws only bite against a ball rolling towards them, so it can still
    // roll in from out of frame at the start of the session.
    if (!leaving && ballVX > 0 && ballX > YARN_STOP) {
      // bumped into the cat — it never rolls up over the body, it just stops
      // dead against the paws, which is the cue to bat it away again
      ballX = YARN_STOP;
      ballVX = 0;
    } else if (ballX <= YARN_OFF) {
      // Gone off the left, past anything we can draw. Nothing bounced it back
      // — it just left, and it will come rolling back on its own.
      if (leaving) endPlay();
      else ballAway = rand(8, 20);
    }
  }

  function yarnFrame(): number {
    return ((Math.floor(ballSpin) % 3) + 3) % 3;
  }

  // --- agent states, driven by the subscription bridge ---

  /** The model is reasoning. */
  export function think() {
    touch();
    stopPlay();
    mood = "think";
    sprites = [];
  }

  /** The agent picked up a tool — the cat bats at the work. */
  export function work() {
    touch();
    stopPlay();
    if (mood !== "work") {
      pawPhase = 0;
      // pace around wherever it happens to be standing now
      homeX = winX;
      walkDir = Math.random() < 0.5 ? -1 : 1;
    }
    mood = "work";
  }

  /** A tool call landed; pop a spark without changing mood. */
  export function spark() {
    sprites.push({
      grid: SPARK,
      x: rand(CAT_X - 2, CAT_X + 22),
      y: rand(CAT_Y - 6, CAT_Y + 2),
      vx: Math.random() < 0.5 ? -1 : 1,
      vy: -1,
      life: rand(8, 14),
    });
  }

  /** Turn finished happily. */
  export function done() {
    touch();
    stopPlay();
    mood = "done";
    reactLeft = 22;
    sprites = [];
    hearts();
  }

  /** Turn failed. */
  export function oops() {
    touch();
    stopPlay();
    mood = "oops";
    reactLeft = 24;
    sprites = [];
    for (let i = 0; i < 3; i++) {
      sprites.push({
        grid: OOPS,
        x: rand(CAT_X, CAT_X + 20),
        y: rand(CAT_Y - 7, CAT_Y - 2),
        vx: Math.random() < 0.5 ? -1 : 1,
        vy: -1,
        life: rand(12, 20),
      });
    }
  }

  /** Drop back to normal cat behaviour. */
  export function rest() {
    if (BUSY.includes(mood)) {
      mood = "idle";
      sprites = [];
    }
  }

  function step() {
    tick++;
    stepAir();
    stepSwing();
    // the ground line can change under us: panel resize, display change,
    // the taskbar being moved or auto-hidden
    if (tick % 25 === 0) void resync(!moving());

    // tail sways in all moods except sleep — twice as fast when there's
    // something to chase, whether that's yarn or a file
    const eager = mood === "play" || mood === "curious";
    if (tick % (eager ? 3 : 6) === 0) tailPhase = (tailPhase + 1) % 4;

    if (mood === "idle") {
      if (blinkLeft > 0) blinkLeft--;
      else if (tick >= blinkAt) {
        blinkLeft = 2;
        blinkAt = tick + rand(20, 60);
      }
      if (gazeLeft > 0) {
        gazeLeft--;
        if (gazeLeft === 0) gaze = 0;
      } else if (Math.random() < 0.006) {
        gaze = Math.random() < 0.5 ? -1 : 1;
        gazeLeft = rand(8, 20);
      }
      if (tick >= groomAt) {
        mood = "groom";
        groomLeft = rand(5, 9) * 6;
        groomAt = tick + rand(300, 700);
      } else if (tick >= playAt) {
        // self-initiated play, so it deliberately does not count as a touch —
        // the cat should still get sleepy on the usual schedule.
        startPlay();
      }
      if (mood === "idle" && tick - lastTouch > sleepAfter) {
        mood = "sleep";
        sleepAfter = rand(1500, 2400);
      }
    } else if (mood === "groom") {
      groomLeft--;
      if (groomLeft <= 0) mood = "idle";
    } else if (mood === "pet") {
      petLeft--;
      // a mid-game cuddle shouldn't end the game
      if (petLeft <= 0) mood = ballOn ? "play" : "idle";
    } else if (mood === "play") {
      if (blinkLeft > 0) blinkLeft--;
      else if (tick >= blinkAt) {
        blinkLeft = 2;
        blinkAt = tick + rand(30, 70);
      }
      stepPlay();
    } else if (mood === "think") {
      // Blink and glance around, but never groom or doze off mid-turn.
      if (blinkLeft > 0) blinkLeft--;
      else if (tick >= blinkAt) {
        blinkLeft = 2;
        blinkAt = tick + rand(20, 60);
      }
      if (gazeLeft > 0) {
        gazeLeft--;
        if (gazeLeft === 0) gaze = 0;
      } else if (Math.random() < 0.03) {
        gaze = Math.random() < 0.5 ? -1 : 1;
        gazeLeft = rand(6, 14);
      }
      if (tick % 9 === 0) {
        sprites.push({
          grid: THINK_DOT,
          x: rand(CAT_X + 2, CAT_X + 18),
          y: CAT_Y - 2,
          vx: Math.random() < 0.5 ? -1 : 1,
          vy: -1,
          life: rand(12, 18),
        });
      }
    } else if (mood === "work") {
      if (tick % 3 === 0) pawPhase = (pawPhase + 1) % 2;
      if (blinkLeft > 0) blinkLeft--;
      else if (tick >= blinkAt) {
        blinkLeft = 2;
        blinkAt = tick + rand(30, 70);
      }
    } else if (mood === "done" || mood === "oops") {
      reactLeft--;
      if (reactLeft <= 0) mood = "idle";
    } else if (mood === "listen") {
      // Sound arriving at the ear: ripples coming *in* from off to the left,
      // which is the only thing separating a cat that's listening to you from
      // one that's saying something.
      if (--listenLeft <= 0) mood = "idle";
      if (tick % 4 === 0) {
        sprites.push({
          grid: tick % 8 === 0 ? EAR_WAVE : EAR_WAVE_SMALL,
          x: CAT_X - rand(3, 8),
          y: CAT_Y - rand(1, 4),
          vx: 1,
          vy: 0,
          life: rand(6, 10),
        });
      }
    } else if (mood === "sleep") {
      if (tick % 24 === 0) {
        sprites.push({
          grid: Math.random() < 0.5 ? ZZ_BIG : ZZ_SMALL,
          x: CAT_X + 22,
          y: CAT_Y - 1,
          vx: 1,
          vy: -1,
          life: rand(18, 28),
        });
      }
    }

    // particles drift on alternate ticks
    for (const s of sprites) {
      s.life--;
      if (tick % 2 === 0) {
        s.y += s.vy;
        if (tick % 4 === 0) s.x += s.vx;
      }
    }
    sprites = sprites.filter((s) => s.life > 0 && s.y > -6);

    keep();
    draw();
  }

  const TAIL_CYCLE = ["down", "mid", "up", "mid"] as const;

  function currentPose(): CatPose {
    const pose: CatPose = { ...IDLE_POSE };
    pose.tail = TAIL_CYCLE[tailPhase];
    pose.gaze = gaze;

    if (mood === "idle") {
      if (blinkLeft > 0) pose.eyes = "closed";
    } else if (mood === "groom") {
      pose.eyes = "closed";
      pose.groomPaw = Math.floor(groomLeft / 3) % 2 === 0 ? "up" : "down";
      pose.gaze = 0;
    } else if (mood === "pet") {
      pose.eyes = "happy";
      pose.bigBlush = true;
      pose.bob = petLeft > 10 ? 1 : 0;
      pose.gaze = 0;
    } else if (mood === "play") {
      // the ball only ever rolls out to the cat's left, so the eyes stay on it
      pose.gaze = -1;
      if (swatLeft > 0) {
        pose.playPaw = "swat";
      } else if (ballAway > 0 || (ballX >= YARN_STOP - 5 && ballVX < 0.35)) {
        // ball drifting into range, or out of frame and due back any moment —
        // either way the cat crouches over the spot, paw cocked, and wiggles
        pose.playPaw = "ready";
        pose.bob = Math.floor(tick / 3) % 2;
      }
      if (blinkLeft > 0) pose.eyes = "closed";
    } else if (mood === "think") {
      // Tail held mid and still — a focused cat stops swishing.
      pose.tail = "mid";
      if (blinkLeft > 0) pose.eyes = "closed";
    } else if (mood === "work") {
      // Trotting along the taskbar: the paw cycle reads as legs stepping, and
      // the body bobs in time with them.
      pose.tail = "up";
      pose.playPaw = pawPhase === 0 ? "ready" : "swat";
      pose.bob = walking() && pawPhase === 1 ? 1 : 0;
      pose.gaze = walking() ? walkDir : 0;
      if (blinkLeft > 0) pose.eyes = "closed";
    } else if (mood === "done") {
      pose.eyes = "happy";
      pose.bigBlush = true;
      pose.tail = "up";
      pose.bob = reactLeft > 16 ? 1 : 0;
      pose.gaze = 0;
    } else if (mood === "oops") {
      pose.eyes = "closed";
      pose.tail = "down";
      pose.gaze = 0;
    } else if (mood === "curious") {
      // Fixed on the file overhead: eyes up and tracking it, tail held high
      // and flicking, one paw cocked, and the wiggle a cat does right before
      // it goes for something. It never blinks — it isn't losing sight of it.
      pose.eyes = "open";
      pose.tail = "up";
      pose.playPaw = "ready";
      pose.gaze = watchX;
      pose.gazeY = -1;
      pose.bob = Math.floor(tick / 3) % 2;
    } else if (mood === "listen") {
      // Sat up and turned to you: ears high, eyes on you and steady, tail held
      // still. A listening cat doesn't swish and doesn't blink you away
      // halfway through your sentence.
      pose.earsUp = true;
      pose.eyes = "open";
      pose.tail = "mid";
      pose.gaze = 0;
    } else if (mood === "sleep") {
      pose.eyes = "closed";
      pose.tail = "down";
      pose.bob = Math.floor(tick / 8) % 2;
      pose.gaze = 0;
    }

    // Off the ground, nothing the mood wanted survives — gravity is doing the
    // posing now, and the whole arc of a pick-up runs through here: dangling,
    // reaching for the floor, folding into it, and springing back up.
    if (carried() || falling || squash > 0) {
      pose.groomPaw = "none";
      pose.playPaw = "none";
      pose.bob = 0;
      pose.gaze = 0;
    }
    if (carried()) {
      // Held by the scruff: limbs hang, and the body swings on the neck.
      pose.posture = "hang";
      pose.lean = Math.round(swing);
      pose.eyes = "open";
      // eyes on where it's headed, and on the drop waiting underneath
      pose.gaze = clamp(Math.round(-swing / 2), -1, 1);
      pose.gazeY = 1;
    } else if (falling) {
      // Twisted upright and reaching down, so the feet get there first.
      pose.posture = "fall";
      pose.tail = "up";
      pose.eyes = "open";
      pose.gazeY = 1;
    } else if (squash > 0) {
      // Landed. Everything folds into the floor for a beat.
      pose.posture = "land";
      pose.tail = "up";
      pose.eyes = "closed";
    } else if (dip > 0) {
      pose.bob = 1; // down off a pounce — knees, not a whole cat
    }
    pose.lift = Math.round(lift);
    // ...and unfolds a pixel past standing before it settles.
    if (rebound > 0 && !falling && !carried()) pose.lift += 1;
    return pose;
  }

  let ctx: CanvasRenderingContext2D;
  let off: HTMLCanvasElement;
  let offCtx: CanvasRenderingContext2D;

  /** Ticks a sprite spends thinning out before it goes, so none just vanish. */
  const SPRITE_FADE = 6;

  function draw() {
    if (!ctx) return;
    const yarn = ballOn
      ? { grid: YARN_FRAMES[yarnFrame()], x: Math.round(ballX), y: Math.round(ballY) }
      : null;
    // The pile goes down first, so a heart drifting past passes in front of it
    // rather than the other way round — the mice are on the floor, not in the
    // air. Never more than PILE has room for; the count is on the panel.
    const pile = PILE.slice(0, Math.min(gifts, PILE.length)).map((at) => ({
      grid: MOUSE,
      x: at.x,
      y: at.y,
    }));
    const puffs = sprites.map((s) => ({
      ...s,
      alpha: Math.min(1, s.life / SPRITE_FADE),
    }));
    const buf = renderScene(currentPose(), [...pile, ...puffs], yarn, coat);
    offCtx.putImageData(new ImageData(buf, SCENE_W, SCENE_H), 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(off, 0, 0, canvas.width, canvas.height);
  }

  // Repaint the moment a new coat is picked, rather than on the next art tick —
  // the collar's swatches should feel like they're dressing the cat directly.
  // The pile rides along: a gift landing, or a pile you just swept, should
  // show up now and not up to a tenth of a second later.
  $effect(() => {
    coat;
    gifts;
    draw();
  });

  // --- pointer handling: small move = pet, drag = pick the cat up ---
  //
  // The drag is done by hand rather than with the OS's `startDragging`,
  // because handing the window to the OS means never being told where it was
  // let go — and the whole point is that the cat drops back to the taskbar.
  //
  // Pointer positions come in CSS pixels, so they're followed by how far they
  // moved rather than where they landed: a delta scaled by the current
  // `devicePixelRatio` is the one reading that survives being dragged onto a
  // monitor with a different DPI, where the CSS-pixel origin shifts under us.
  let downX = 0;
  let downY = 0;
  let lastX = 0;
  let lastY = 0;
  let pressed = false;
  let dragging = false;

  function onPointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    pressed = true;
    dragging = false;
    downX = lastX = e.screenX;
    downY = lastY = e.screenY;
    canvas.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!pressed) return;
    const dx = e.screenX - lastX;
    const dy = e.screenY - lastY;
    lastX = e.screenX;
    lastY = e.screenY;

    if (
      !dragging &&
      Math.abs(e.screenX - downX) + Math.abs(e.screenY - downY) <= DRAG_SLOP
    ) {
      return;
    }
    if (!dragging) {
      dragging = true;
      touch();
      // scruffed off the floor: it starts swinging on the way up, and which
      // way is whichever way it happened to be leaning
      swingV = Math.random() < 0.5 ? -1.2 : 1.2;
      canvas.style.cursor = "grabbing";
    }
    // Fed to the pendulum on the next art tick, so a fast haul across the desk
    // throws the body further than a careful one.
    carriedDX += dx;
    if (!world || !limits) return;

    // The cat can be carried across every monitor, but never below the ground
    // or off the sides of the one it's over — it stays on its strip of screen.
    const dpr = window.devicePixelRatio || 1;
    winX += dx * dpr;
    winY += dy * dpr;
    limits = perch.ground(world, perch.under(world, winX, winY, limits.screen));
    winX = clamp(winX, limits.minX, limits.maxX);
    winY = clamp(winY, limits.ceiling, limits.floor);
    fallVY = 0;
    lift = 0;
    liftV = 0;
    place();
  }

  function onPointerUp(e: PointerEvent) {
    if (e.button !== 0) return;
    if (canvas.hasPointerCapture(e.pointerId)) {
      canvas.releasePointerCapture(e.pointerId);
    }
    if (pressed && !dragging) pet();
    pressed = false;
    // let go — from here gravity takes over in stepGround()
    drop();
  }

  /** Pointer lost (window focus stolen, touch cancelled) — drop the cat. */
  function onPointerCancel() {
    pressed = false;
    drop();
  }

  /**
   * Out of the hand. A cat dropped from height stays limp for a beat before it
   * twists itself feet-down — put back on the floor it just stands up, because
   * there is nothing to fall through.
   */
  function drop() {
    if (!dragging) return;
    dragging = false;
    canvas.style.cursor = "";
    limp = limits && winY < limits.floor - 1 ? LIMP_TICKS : 0;
    if (limp === 0) {
      swing = 0;
      swingV = 0;
    }
  }

  onMount(() => {
    ctx = canvas.getContext("2d")!;
    off = document.createElement("canvas");
    off.width = SCENE_W;
    off.height = SCENE_H;
    offCtx = off.getContext("2d")!;
    draw();
    void resync();
    void restore();

    const id = setInterval(step, TICK_MS);

    let raf = 0;
    let last = 0;
    const frame = (now: number) => {
      raf = requestAnimationFrame(frame);
      // clamped so a backgrounded window doesn't resume with a huge step
      const dt = last ? Math.min((now - last) / 1000, 0.1) : 0;
      last = now;
      if (dt > 0) stepGround(dt);
    };
    raf = requestAnimationFrame(frame);

    return () => {
      clearInterval(id);
      cancelAnimationFrame(raf);
    };
  });
</script>

<canvas
  bind:this={canvas}
  width={SCENE_W * SCALE}
  height={SCENE_H * SCALE}
  style="width: {SCENE_W * SCALE}px; height: {SCENE_H * SCALE}px;"
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerCancel}
  ondblclick={() => ondblclick?.()}
></canvas>

<style>
  canvas {
    display: block;
    image-rendering: pixelated;
    cursor: grab;
  }
</style>
