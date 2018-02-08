from __future__ import print_function
import socket
import time
from contextlib import closing

def main():
  host = '127.0.0.1'
  port = 10001
  count = 0.0
  sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
  with closing(sock):
    while True:
      message = "Hello world : {0}".format(count/10).encode('utf-8')
      print('send: ' + message)
      sock.sendto(message, (host, port))
      count += 1
      time.sleep(0.1)
  return

if __name__ == '__main__':
  main()
