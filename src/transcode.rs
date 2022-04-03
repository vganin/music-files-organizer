use std::path::Path;

use ffmpeg::{codec, filter, format, frame, media};
use ffmpeg::Dictionary;

pub fn to_mp4(input: &Path, output: &Path) {
    transcode(
        input,
        output,
        "mp4",
        "libfdk_aac",
        [("cutoff", "20000"), ("afterburner", "1")],
    );
}

fn transcode<'a, T: IntoIterator<Item=(&'a str, &'a str)>>(
    input: &Path,
    output: &Path,
    output_format: &str,
    output_codec: &str,
    output_extra_options: T,
) {
    ffmpeg::init().unwrap();

    let mut input_format = format::input(&input).unwrap();
    let mut output_format = format::output_as(&output, format::output::by_name(output_format).next().unwrap()).unwrap();
    let mut transcoder = transcoder(&mut input_format, &mut output_format, output_codec, output_extra_options, "anull").unwrap();

    output_format.set_metadata(input_format.metadata().to_owned());
    output_format.write_header().unwrap();

    for res in input_format.packets() {
        let (stream, mut packet) = res.unwrap();
        if stream.index() == transcoder.stream {
            packet.rescale_ts(stream.time_base(), transcoder.in_time_base);
            transcoder.send_packet_to_decoder(&packet);
            transcoder.receive_and_process_decoded_frames(&mut output_format);
        }
    }

    transcoder.send_eof_to_decoder();
    transcoder.receive_and_process_decoded_frames(&mut output_format);

    transcoder.flush_filter();
    transcoder.get_and_process_filtered_frames(&mut output_format);

    transcoder.send_eof_to_encoder();
    transcoder.receive_and_process_encoded_packets(&mut output_format);

    output_format.write_trailer().unwrap();
}

fn transcoder<'a>(
    input_format: &mut format::context::Input,
    output_format: &mut format::context::Output,
    output_codec: &str,
    output_codec_options: impl IntoIterator<Item=(&'a str, &'a str)>,
    filter_spec: &str,
) -> Result<Transcoder, ffmpeg::Error> {
    let input = input_format
        .streams()
        .best(media::Type::Audio)
        .expect("could not find best audio stream");
    let mut decoder = input.codec().decoder().audio()?;
    let codec = ffmpeg::encoder::find_by_name(output_codec)
        .expect("failed to find encoder")
        .audio()?;
    let global = output_format
        .format()
        .flags()
        .contains(ffmpeg::format::flag::Flags::GLOBAL_HEADER);

    decoder.set_parameters(input.parameters())?;

    let mut output = output_format.add_stream(codec)?;
    let mut encoder = output.codec().encoder().audio()?;

    let channel_layout = codec
        .channel_layouts()
        .map(|cls| cls.best(decoder.channel_layout().channels()))
        .unwrap_or(ffmpeg::channel_layout::ChannelLayout::STEREO);

    if global {
        encoder.set_flags(ffmpeg::codec::flag::Flags::GLOBAL_HEADER);
    }

    encoder.set_sample_rate(decoder.sample_rate());
    encoder.set_channel_layout(channel_layout);
    encoder.set_channels(channel_layout.channels());
    encoder.set_format(codec.formats().expect("unknown supported formats").next().unwrap());
    encoder.set_bit_rate(decoder.bit_rate());
    encoder.set_max_bit_rate(decoder.max_bit_rate());

    encoder.set_time_base((1, decoder.sample_rate() as i32));
    output.set_time_base((1, decoder.sample_rate() as i32));

    let encoder = encoder.open_as_with(codec, Dictionary::from_iter(output_codec_options))?;
    output.set_parameters(&encoder);

    let filter = filter(filter_spec, &decoder, &encoder)?;

    let in_time_base = decoder.time_base();
    let out_time_base = output.time_base();

    Ok(Transcoder {
        stream: input.index(),
        filter,
        decoder,
        encoder,
        in_time_base,
        out_time_base,
    })
}

fn filter(
    spec: &str,
    decoder: &codec::decoder::Audio,
    encoder: &codec::encoder::Audio,
) -> Result<filter::Graph, ffmpeg::Error> {
    let mut filter = filter::Graph::new();

    let args = format!(
        "time_base={}:sample_rate={}:sample_fmt={}:channel_layout=0x{:x}",
        decoder.time_base(),
        decoder.sample_rate(),
        decoder.format().name(),
        decoder.channel_layout().bits()
    );

    filter.add(&filter::find("abuffer").unwrap(), "in", &args)?;
    filter.add(&filter::find("abuffersink").unwrap(), "out", "")?;

    {
        let mut out = filter.get("out").unwrap();

        out.set_sample_format(encoder.format());
        out.set_channel_layout(encoder.channel_layout());
        out.set_sample_rate(encoder.sample_rate());
    }

    filter.output("in", 0)?.input("out", 0)?.parse(spec)?;
    filter.validate()?;

    if let Some(codec) = encoder.codec() {
        if !codec
            .capabilities()
            .contains(ffmpeg::codec::capabilities::Capabilities::VARIABLE_FRAME_SIZE)
        {
            filter.get("out").unwrap().sink().set_frame_size(encoder.frame_size());
        }
    }

    Ok(filter)
}

struct Transcoder {
    stream: usize,
    filter: filter::Graph,
    decoder: codec::decoder::Audio,
    encoder: codec::encoder::Audio,
    in_time_base: ffmpeg::Rational,
    out_time_base: ffmpeg::Rational,
}

impl Transcoder {
    fn send_frame_to_encoder(&mut self, frame: &ffmpeg::Frame) {
        self.encoder.send_frame(frame).unwrap();
    }

    fn send_eof_to_encoder(&mut self) {
        self.encoder.send_eof().unwrap();
    }

    fn receive_and_process_encoded_packets(&mut self, octx: &mut format::context::Output) {
        let mut encoded = ffmpeg::Packet::empty();
        while self.encoder.receive_packet(&mut encoded).is_ok() {
            encoded.set_stream(0);
            encoded.rescale_ts(self.in_time_base, self.out_time_base);
            encoded.write_interleaved(octx).unwrap();
        }
    }

    fn add_frame_to_filter(&mut self, frame: &ffmpeg::Frame) {
        self.filter.get("in").unwrap().source().add(frame).unwrap();
    }

    fn flush_filter(&mut self) {
        self.filter.get("in").unwrap().source().flush().unwrap();
    }

    fn get_and_process_filtered_frames(&mut self, octx: &mut format::context::Output) {
        let mut filtered = frame::Audio::empty();
        while self.filter.get("out").unwrap().sink().frame(&mut filtered).is_ok() {
            self.send_frame_to_encoder(&filtered);
            self.receive_and_process_encoded_packets(octx);
        }
    }

    fn send_packet_to_decoder(&mut self, packet: &ffmpeg::Packet) {
        self.decoder.send_packet(packet).unwrap();
    }

    fn send_eof_to_decoder(&mut self) {
        self.decoder.send_eof().unwrap();
    }

    fn receive_and_process_decoded_frames(&mut self, octx: &mut format::context::Output) {
        let mut decoded = frame::Audio::empty();
        while self.decoder.receive_frame(&mut decoded).is_ok() {
            let timestamp = decoded.timestamp();
            decoded.set_pts(timestamp);
            self.add_frame_to_filter(&decoded);
            self.get_and_process_filtered_frames(octx);
        }
    }
}
