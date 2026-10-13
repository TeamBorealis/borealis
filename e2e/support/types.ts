// Structural slices of the e2e fixtures, so helpers do not depend on unexported type names.
export interface Box { x: number; y: number; width: number; height: number }
export interface LocatorLike {
  boundingBox(): Promise<Box | null>;
  waitFor(opts?: { state?: 'attached' | 'detached' | 'visible' | 'hidden'; timeout?: number }): Promise<void>;
}
export interface ScreenLike { tapAt(point: { x: number; y: number }): Promise<void> }
export interface BrowserLike {
  evaluate<R, A>(fn: (arg: A) => R, arg: A): Promise<R>;
}
