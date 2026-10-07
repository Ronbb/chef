# 录音协议测试素材

`synthetic.mp3` 是本机 FFmpeg 生成的一秒 440 Hz 合成正弦波，单声道、24 kHz、48 kbps MP3。没有第三方录音或声音表演，不作为教学内容、法语发音或授权审校的证明。

生成命令：

```sh
ffmpeg -hide_banner -loglevel error -f lavfi -i 'sine=frequency=440:sample_rate=24000:duration=1' -ac 1 -c:a libmp3lame -b:a 48k -map_metadata -1 -y synthetic.mp3
```

测试不要求安装 FFmpeg，直接解码已提交的文件；WAV 测试在内存生成 PCM 数据。
