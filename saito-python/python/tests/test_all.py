import pytest
import pytest_asyncio
import saito_python
import logging


def test_sum_as_string():
    assert saito_python.sum_as_string(1, 1) == "2"


@pytest.mark.asyncio
async def test_init():
    print("111")
    FORMAT = '%(levelname)s %(name)s %(asctime)-15s %(filename)s:%(lineno)d %(message)s'
    logging.basicConfig(format=FORMAT)
    logging.getLogger().setLevel(logging.INFO)
    
    await saito_python.initialize()
    assert False
