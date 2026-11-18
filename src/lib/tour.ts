const DONE_KEY = "tour-done";

export function tourDone(): boolean {
  try {
    return localStorage.getItem(DONE_KEY) === "1";
  } catch {
    return true;
  }
}

export function markTourDone() {
  try {
    localStorage.setItem(DONE_KEY, "1");
  } catch {
  }
}
