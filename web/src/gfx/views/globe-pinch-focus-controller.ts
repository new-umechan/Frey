import * as THREE from "three";

const WHEEL_ZOOM_SENSITIVITY = 0.0015;
const ROTATE_GAIN = 0.18;
const MAX_ROTATE_STEP_RAD = 0.075;
const EPSILON = 1e-6;
const MIN_POINTERS_FOR_PINCH = 2;

export interface GlobePinchFocusController {
    reset: () => void;
    update: () => void;
    onPointerDown: (event: PointerEvent) => void;
    onPointerMove: (event: PointerEvent) => void;
    onPointerUp: (event: PointerEvent) => void;
    onPointerCancel: (event: PointerEvent) => void;
    onWheel: (event: WheelEvent) => boolean;
}

export function createGlobePinchFocusController({
    canvas,
    sphere,
    globeCamera,
    globeControls,
    getCurrentSurfaceMode,
}: {
    canvas: HTMLCanvasElement;
    sphere: THREE.Mesh;
    globeCamera: THREE.PerspectiveCamera;
    globeControls: any; // TODO: improve this type if possible
    getCurrentSurfaceMode: () => string;
}): GlobePinchFocusController {
    const activePointers = new Map();
    const raycaster = new THREE.Raycaster();
    const pointerNdc = new THREE.Vector2();
    const sphereCenter = new THREE.Vector3();
    const currentDirection = new THREE.Vector3();
    const desiredDirection = new THREE.Vector3();
    const reverseDirection = new THREE.Vector3();
    const nextDirection = new THREE.Vector3();
    const nextPosition = new THREE.Vector3();
    const centerSurfaceDirection = new THREE.Vector3();
    const anchorSurfaceDirection = new THREE.Vector3();
    const rotateQuat = new THREE.Quaternion();
    let pinchDistance = null;
    let previousNoRotate = false;

    function isTouchEvent(event) {
        return event.pointerType === "touch";
    }

    function clearPinchState() {
        pinchDistance = null;
        globeControls.noRotate = previousNoRotate;
    }

    function removePointer(pointerId) {
        activePointers.delete(pointerId);
        if (activePointers.size < MIN_POINTERS_FOR_PINCH) {
            clearPinchState();
        }
    }

    function reset() {
        activePointers.clear();
        clearPinchState();
    }

    function getPinchDistance(firstPointer, secondPointer) {
        const dx = firstPointer.clientX - secondPointer.clientX;
        const dy = firstPointer.clientY - secondPointer.clientY;
        return Math.hypot(dx, dy);
    }

    function raycastGlobeAtNdc(ndcX, ndcY) {
        pointerNdc.set(ndcX, ndcY);
        raycaster.setFromCamera(pointerNdc, globeCamera);
        const [hit] = raycaster.intersectObject(sphere, false);
        return hit?.point ?? null;
    }

    function raycastGlobe(clientX, clientY) {
        const rect = canvas.getBoundingClientRect();
        if (rect.width <= 0 || rect.height <= 0) {
            return null;
        }
        const ndcX = ((clientX - rect.left) / rect.width) * 2 - 1;
        const ndcY = -((clientY - rect.top) / rect.height) * 2 + 1;
        return raycastGlobeAtNdc(ndcX, ndcY);
    }

    function applyCenterZoomOnly(currentDistance, zoomRatio) {
        const nextDistance = THREE.MathUtils.clamp(
            currentDistance * zoomRatio,
            globeControls.minDistance,
            globeControls.maxDistance,
        );
        currentDirection.normalize();
        nextPosition.copy(sphereCenter).addScaledVector(currentDirection, nextDistance);
        globeCamera.position.copy(nextPosition);
        globeControls.target.copy(sphereCenter);
        globeCamera.lookAt(sphereCenter);
    }

    function applyFocusZoom(clientX, clientY, zoomRatio = 1) {
        if (getCurrentSurfaceMode() !== "globe") {
            return;
        }

        sphere.getWorldPosition(sphereCenter);
        currentDirection.subVectors(globeCamera.position, sphereCenter);
        const currentDistance = currentDirection.length();
        if (!Number.isFinite(currentDistance) || currentDistance <= 1e-6) {
            return;
        }

        const anchorPoint = raycastGlobe(clientX, clientY);
        const centerPoint = raycastGlobeAtNdc(0, 0);
        if (!anchorPoint || !centerPoint) {
            applyCenterZoomOnly(currentDistance, zoomRatio);
            return;
        }

        desiredDirection.subVectors(anchorPoint, sphereCenter).normalize();
        const normalizedCurrentDirection = currentDirection.normalize();
        const isZoomOut = zoomRatio > 1;
        let targetDirection = desiredDirection;
        if (isZoomOut) {
            reverseDirection
                .copy(normalizedCurrentDirection)
                .multiplyScalar(2)
                .sub(desiredDirection)
                .normalize();
            targetDirection = reverseDirection;
        }
        const cosAngle = THREE.MathUtils.clamp(
            normalizedCurrentDirection.dot(targetDirection),
            -1,
            1,
        );
        const angle = Math.acos(cosAngle);

        centerSurfaceDirection.subVectors(centerPoint, sphereCenter);
        anchorSurfaceDirection.subVectors(anchorPoint, sphereCenter);
        const centerRadius = centerSurfaceDirection.length();
        const anchorRadius = anchorSurfaceDirection.length();
        const sphereRadius = Math.max(
            Number.isFinite(centerRadius) ? centerRadius : 0,
            Number.isFinite(anchorRadius) ? anchorRadius : 0,
            EPSILON,
        );
        centerSurfaceDirection.normalize();
        anchorSurfaceDirection.normalize();
        const rhoAngle = Math.acos(THREE.MathUtils.clamp(centerSurfaceDirection.dot(anchorSurfaceDirection), -1, 1));
        const rho = sphereRadius * rhoAngle;

        const fovRad = THREE.MathUtils.degToRad(globeCamera.fov ?? 50);
        const f = 1 / Math.max(Math.tan(fovRad * 0.5), EPSILON);
        const cosTheta = Math.cos(angle);
        const j = f / Math.max(cosTheta * cosTheta, EPSILON);
        const deltaTheta = THREE.MathUtils.clamp(
            ROTATE_GAIN * (rho / sphereRadius) / Math.max(j, EPSILON),
            0,
            MAX_ROTATE_STEP_RAD,
        );
        const stepGain = Math.min(1, deltaTheta / Math.max(angle, EPSILON));

        nextDirection.copy(normalizedCurrentDirection).lerp(targetDirection, stepGain);
        if (nextDirection.lengthSq() <= 1e-8) {
            nextDirection.copy(normalizedCurrentDirection);
        } else {
            nextDirection.normalize();
        }

        const nextDistance = THREE.MathUtils.clamp(
            currentDistance * zoomRatio,
            globeControls.minDistance,
            globeControls.maxDistance,
        );
        nextPosition.copy(sphereCenter).addScaledVector(nextDirection, nextDistance);

        rotateQuat.setFromUnitVectors(normalizedCurrentDirection, nextDirection);
        if (
            Number.isFinite(rotateQuat.x)
            && Number.isFinite(rotateQuat.y)
            && Number.isFinite(rotateQuat.z)
            && Number.isFinite(rotateQuat.w)
        ) {
            globeCamera.up.applyQuaternion(rotateQuat).normalize();
        }
        globeCamera.position.copy(nextPosition);
        globeControls.target.copy(sphereCenter);
        globeCamera.lookAt(sphereCenter);
    }

    function onPointerDown(event) {
        if (!isTouchEvent(event)) {
            return;
        }
        activePointers.set(event.pointerId, {
            clientX: event.clientX,
            clientY: event.clientY,
        });
        if (activePointers.size === MIN_POINTERS_FOR_PINCH) {
            const pointers = Array.from(activePointers.values());
            pinchDistance = getPinchDistance(pointers[0], pointers[1]);
            previousNoRotate = globeControls.noRotate;
            globeControls.noRotate = true;
        }
    }

    function onPointerMove(event) {
        if (!isTouchEvent(event)) {
            return;
        }
        if (!activePointers.has(event.pointerId)) {
            return;
        }
        activePointers.set(event.pointerId, {
            clientX: event.clientX,
            clientY: event.clientY,
        });
        if (activePointers.size < MIN_POINTERS_FOR_PINCH) {
            return;
        }

        const pointers = Array.from(activePointers.values());
        const currentPinchDistance = getPinchDistance(pointers[0], pointers[1]);
        if (!Number.isFinite(currentPinchDistance) || currentPinchDistance <= 0) {
            return;
        }
        const prevDistance = pinchDistance ?? currentPinchDistance;
        const zoomRatio = prevDistance / currentPinchDistance;
        pinchDistance = currentPinchDistance;
        const midX = (pointers[0].clientX + pointers[1].clientX) * 0.5;
        const midY = (pointers[0].clientY + pointers[1].clientY) * 0.5;
        applyFocusZoom(midX, midY, zoomRatio);
    }

    function onPointerUp(event) {
        if (!isTouchEvent(event)) {
            return;
        }
        removePointer(event.pointerId);
    }

    function onPointerCancel(event) {
        if (!isTouchEvent(event)) {
            return;
        }
        removePointer(event.pointerId);
    }

    function onWheel(event) {
        if (getCurrentSurfaceMode() !== "globe") {
            return false;
        }
        const zoomRatio = Math.exp(event.deltaY * WHEEL_ZOOM_SENSITIVITY);
        applyFocusZoom(event.clientX, event.clientY, zoomRatio);
        return true;
    }

    function update() {
        if (getCurrentSurfaceMode() !== "globe") {
            return;
        }
        sphere.getWorldPosition(sphereCenter);
        globeControls.target.copy(sphereCenter);
    }

    return {
        reset,
        update,
        onPointerDown,
        onPointerMove,
        onPointerUp,
        onPointerCancel,
        onWheel,
    };
}
