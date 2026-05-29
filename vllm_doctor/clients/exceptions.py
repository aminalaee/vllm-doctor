class ClientError(Exception):
    pass


class ClientConnectionError(ClientError):
    pass


class ClientQueryError(ClientError):
    pass
