"""Unit tests for TrayDrop (POSIX path; Windows mutex path is exercised CI-side)."""

from __future__ import annotations

import os
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "src"))

from traydrop.single_instance import AlreadyRunning, SingleInstance  # noqa: E402
from traydrop.tray import make_icon_image  # noqa: E402


@pytest.mark.skipif(sys.platform == "win32", reason="flock-based path")
def test_second_instance_detects_first(tmp_path, monkeypatch):
    monkeypatch.setattr(
        SingleInstance, "_lock_path", lambda self: tmp_path / "t.lock"
    )
    monkeypatch.setattr(
        SingleInstance, "_pid_path", lambda self: tmp_path / "t.pid"
    )
    first = SingleInstance("test-traydrop")
    first.acquire()
    try:
        assert int((tmp_path / "t.pid").read_text()) == os.getpid()
        second = SingleInstance("test-traydrop")
        with pytest.raises(AlreadyRunning):
            second.acquire()
    finally:
        first.release()


@pytest.mark.skipif(sys.platform == "win32", reason="flock-based path")
def test_release_allows_reacquire(tmp_path, monkeypatch):
    monkeypatch.setattr(
        SingleInstance, "_lock_path", lambda self: tmp_path / "t.lock"
    )
    monkeypatch.setattr(
        SingleInstance, "_pid_path", lambda self: tmp_path / "t.pid"
    )
    a = SingleInstance("test-traydrop")
    a.acquire()
    a.release()
    b = SingleInstance("test-traydrop")
    b.acquire()   # must not raise
    b.release()


def test_icon_image_is_rendered():
    img = make_icon_image()
    assert img.size == (64, 64)
    assert img.mode == "RGBA"
    # corner pixel transparent, centre coloured
    assert img.getpixel((0, 0))[3] == 0
    assert img.getpixel((32, 32))[3] == 255
