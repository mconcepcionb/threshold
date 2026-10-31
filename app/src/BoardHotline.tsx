import { useCallback, useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { speakBoardLine, type BoardVoiceHandle } from "./boardVoice";

const BOARD_TEXT = "YOU ARE AUTHORITY / CHOSEN ONE";

/**
 * The Board's Hotline: a white Astral Plane, an inverted black pyramid, and
 * one transmission. The scene is deliberately slow; the text is the event.
 *
 * This is a separate surface from the cue Card. It owns an opaque white window,
 * so it never has to fight the desktop for legibility, and it is lazy-loaded so
 * the cue overlay never pays for three.js.
 */
export default function BoardHotline() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [fallback, setFallback] = useState(false);
  const [listening, setListening] = useState(false);
  const voiceRef = useRef<BoardVoiceHandle | null>(null);
  const listenTimer = useRef<number | null>(null);

  const stopListening = useCallback(() => {
    if (listenTimer.current !== null) {
      window.clearTimeout(listenTimer.current);
      listenTimer.current = null;
    }
    voiceRef.current?.stop();
    voiceRef.current = null;
    setListening(false);
  }, []);

  useEffect(() => () => stopListening(), [stopListening]);

  const toggleListening = async () => {
    if (listening) {
      stopListening();
      return;
    }
    try {
      const voice = await speakBoardLine(BOARD_TEXT);
      voiceRef.current = voice;
      setListening(true);
      listenTimer.current = window.setTimeout(() => {
        voiceRef.current?.stop();
        voiceRef.current = null;
        listenTimer.current = null;
        setListening(false);
      }, voice.duration * 1000 + 250);
    } catch (error) {
      console.error("Board voice could not start", error);
      setListening(false);
    }
  };

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({
        canvas,
        antialias: true,
        alpha: false,
        powerPreference: "high-performance",
      });
    } catch {
      setFallback(true);
      return;
    }

    const startedAt = performance.now();
    const scene = new THREE.Scene();
    const paper = 0xf7f5ef;
    scene.background = new THREE.Color(paper);
    scene.fog = new THREE.Fog(paper, 7.5, 24);

    const camera = new THREE.PerspectiveCamera(32, 1, 0.1, 100);
    camera.position.set(0, 0.35, 10.6);
    camera.lookAt(0, 0.25, 0);

    renderer.outputColorSpace = THREE.SRGBColorSpace;

    scene.add(new THREE.HemisphereLight(0xffffff, 0xffffff, 1.5));

    const key = new THREE.DirectionalLight(0xffffff, 2.5);
    key.position.set(3.5, 6, 4);
    scene.add(key);

    const fill = new THREE.DirectionalLight(0xffffff, 0.55);
    fill.position.set(-5, 1.5, 3);
    scene.add(fill);

    // The Board. An open inverted pyramid with no cap and no base: the sides
    // leave the frame and run into the same scene fog as everything else.
    const PYRAMID_HEIGHT = 22;
    const PYRAMID_RADIUS = 15;
    const apexY = -0.55;
    const vertices = new Float32Array([
      0, 0, 0, PYRAMID_RADIUS, PYRAMID_HEIGHT, PYRAMID_RADIUS, -PYRAMID_RADIUS, PYRAMID_HEIGHT, PYRAMID_RADIUS,
      0, 0, 0, PYRAMID_RADIUS, PYRAMID_HEIGHT, -PYRAMID_RADIUS, PYRAMID_RADIUS, PYRAMID_HEIGHT, PYRAMID_RADIUS,
      0, 0, 0, -PYRAMID_RADIUS, PYRAMID_HEIGHT, PYRAMID_RADIUS, -PYRAMID_RADIUS, PYRAMID_HEIGHT, -PYRAMID_RADIUS,
      0, 0, 0, -PYRAMID_RADIUS, PYRAMID_HEIGHT, -PYRAMID_RADIUS, PYRAMID_RADIUS, PYRAMID_HEIGHT, -PYRAMID_RADIUS,
    ]);
    const pyramidGeometry = new THREE.BufferGeometry();
    pyramidGeometry.setAttribute("position", new THREE.BufferAttribute(vertices, 3));
    pyramidGeometry.computeVertexNormals();
    const pyramidMaterial = new THREE.MeshStandardMaterial({
      color: 0x070707,
      roughness: 0.92,
      metalness: 0.02,
      flatShading: true,
      side: THREE.FrontSide,
    });
    const pyramid = new THREE.Mesh(pyramidGeometry, pyramidMaterial);
    pyramid.position.set(0.25, apexY, 0);
    scene.add(pyramid);

    // Floating islands: blocky, flat-topped, with steps and a monolith. They sit
    // at the edges so the Board's type never has to compete with them.
    const islandTop = new THREE.MeshStandardMaterial({
      color: 0xd2cec3,
      roughness: 0.96,
      metalness: 0,
      flatShading: true,
    });
    const islandSide = new THREE.MeshStandardMaterial({
      color: 0x8d897f,
      roughness: 0.98,
      metalness: 0,
      flatShading: true,
    });
    const islandBottom = new THREE.MeshStandardMaterial({
      color: 0x1b1a17,
      roughness: 1,
      metalness: 0,
      flatShading: true,
    });
    const monolithMaterial = new THREE.MeshStandardMaterial({
      color: 0x080808,
      roughness: 0.9,
      metalness: 0,
      flatShading: true,
    });

    const block = (w: number, h: number, d: number) =>
      new THREE.Mesh(new THREE.BoxGeometry(w, h, d), [
        islandSide,
        islandSide,
        islandTop,
        islandBottom,
        islandSide,
        islandSide,
      ]);

    const makeIsland = (x: number, y: number, z: number, s: number) => {
      const group = new THREE.Group();
      const plate = block(3.6 * s, 0.42 * s, 2.7 * s);
      plate.position.y = 0;
      group.add(plate);

      const step1 = block(1.25 * s, 0.22 * s, 0.95 * s);
      step1.position.set(-1.0 * s, 0.31 * s, 0.45 * s);
      group.add(step1);

      const step2 = block(0.85 * s, 0.22 * s, 0.8 * s);
      step2.position.set(-1.65 * s, 0.5 * s, 0.45 * s);
      group.add(step2);

      const wall = block(1.55 * s, 0.5 * s, 0.16 * s);
      wall.position.set(0.75 * s, 0.46 * s, -1.05 * s);
      group.add(wall);

      const monolith = new THREE.Mesh(
        new THREE.BoxGeometry(0.22 * s, 1.25 * s, 0.22 * s),
        monolithMaterial
      );
      monolith.position.set(1.15 * s, 0.82 * s, 0.65 * s);
      group.add(monolith);

      const under1 = new THREE.Mesh(
        new THREE.BoxGeometry(2.3 * s, 0.5 * s, 1.8 * s),
        islandBottom
      );
      under1.position.y = -0.45 * s;
      group.add(under1);

      const under2 = new THREE.Mesh(
        new THREE.BoxGeometry(1.2 * s, 0.6 * s, 0.95 * s),
        islandBottom
      );
      under2.position.set(0.15 * s, -0.9 * s, 0.1 * s);
      group.add(under2);

      group.position.set(x, y, z);
      group.userData.baseY = y;
      scene.add(group);
      return group;
    };

    makeIsland(-4.8, -2.1, 0.4, 1.0);
    makeIsland(4.8, -2.2, -0.8, 0.9);
    const farLeft = makeIsland(-6.2, -1.7, -10.0, 0.8);
    const farRight = makeIsland(6.2, -1.6, -10.5, 0.7);
    const floaters = [farLeft, farRight];

    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    const resize = () => {
      const width = window.innerWidth;
      const height = window.innerHeight;
      const aspect = width / height;
      camera.aspect = aspect;
      // Keep the pyramid in the upper half on portrait screens.
      camera.position.z = aspect < 1 ? 13.8 : 10.6;
      camera.updateProjectionMatrix();
      renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
      renderer.setSize(width, height, false);
      if (reduceMotion) renderer.render(scene, camera);
    };

    let raf = 0;
    let disposed = false;

    const animate = () => {
      if (disposed) return;
      const t = (performance.now() - startedAt) / 1000;
      pyramid.position.y = apexY + Math.sin(t * 0.42) * 0.05;
      pyramid.rotation.y = t * 0.055;
      camera.position.x = Math.sin(t * 0.07) * 0.18;
      camera.position.y = 0.35 + Math.sin(t * 0.1) * 0.045;
      camera.lookAt(0.1, 0.25, 0);
      for (let i = 0; i < floaters.length; i++) {
        floaters[i].position.y =
          floaters[i].userData.baseY + Math.sin(t * 0.24 + i * 1.7) * 0.06;
      }
      renderer.render(scene, camera);
      raf = requestAnimationFrame(animate);
    };

    window.addEventListener("resize", resize);
    resize();
    if (reduceMotion) renderer.render(scene, camera);
    else animate();

    return () => {
      disposed = true;
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", resize);
      scene.traverse((object) => {
        const mesh = object as THREE.Mesh;
        if (mesh.geometry) mesh.geometry.dispose();
        const material = mesh.material as THREE.Material | THREE.Material[] | undefined;
        if (Array.isArray(material)) material.forEach((m) => m.dispose());
        else material?.dispose();
      });
      renderer.dispose();
    };
  }, []);

  return (
    <main className="board-shell">
      <canvas
        ref={canvasRef}
        className={`board-canvas${fallback ? " hidden" : ""}`}
        aria-hidden="true"
      />
      {fallback && <div className="board-fallback" aria-hidden="true" />}
      <div className="board-grain" aria-hidden="true" />

      <div className="board-overlay">
        <header className="board-top">
          <span className="board-dot" />
          <span className="board-kicker">HOTLINE</span>
          <span className="board-caller">
            <b>THE BOARD</b> · TRANSMISSION 00:12
          </span>
        </header>

        <section className="board-copy" aria-label="Hotline transmission">
          <p className="board-line">
            <span className="board-angle">&lt;</span> YOU ARE AUTHORITY{" "}
            <span className="board-slash">/</span>
            <br />
            CHOSEN ONE <span className="board-angle">&gt;</span>
          </p>
          <p className="board-ask">A response is required</p>
        </section>

        <footer className="board-actions">
          <button
            type="button"
            className={`board-btn${listening ? " live" : ""}`}
            aria-pressed={listening}
            onClick={() => void toggleListening()}
          >
            {listening ? "STOP" : "LISTEN"}
          </button>
          <button type="button" className="board-btn primary">
            ACKNOWLEDGE
          </button>
          <button type="button" className="board-btn">
            HANG UP
          </button>
          <span className="board-hint">
            ONE-WAY TRANSMISSION
            <br />
            NOT A CONVERSATION
          </span>
        </footer>
      </div>
    </main>
  );
}
