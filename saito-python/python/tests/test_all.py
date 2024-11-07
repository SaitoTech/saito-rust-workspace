import pytest
import saito_python


def test_sum_as_string():
    assert saito_python.sum_as_string(1, 1) == "2"


def test_init():
    print("test 123")
    assert saito_python.initialize()
