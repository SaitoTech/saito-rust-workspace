import pytest
import pytest_asyncio
import saito_python


def test_sum_as_string():
    assert saito_python.sum_as_string(1, 1) == "2"


@pytest.mark.asyncio
async def test_init():
    print("111")
    await saito_python.initialize()
    assert False
