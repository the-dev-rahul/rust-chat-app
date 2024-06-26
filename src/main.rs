use std::error::Error;
use std::time::Duration;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

use libp2p::futures::StreamExt;
use libp2p::{gossipsub, mdns, noise, swarm::NetworkBehaviour, swarm::SwarmEvent, tcp, yamux};
use tokio::{io, io::AsyncBufReadExt, select};



#[derive(NetworkBehaviour)]
struct MyBehaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>>{

    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
    .with_tokio()
    .with_tcp(
        tcp::Config::default(),
        noise::Config::new,
        yamux::Config::default,
    )?
    .with_quic()
    .with_behaviour(|key| {

        let message_id_fn = |message: &gossipsub::Message| {
            let mut s = DefaultHasher::new();
            message.data.hash(&mut s);
            gossipsub::MessageId::from(s.finish().to_string())
        };


        let gossipsub_config = gossipsub::ConfigBuilder::default()
        // .duplicate_cache_time(Duration::from_secs(0))
        .heartbeat_interval(Duration::from_secs(10)) 
        .validation_mode(gossipsub::ValidationMode::Strict) 
        .message_id_fn(message_id_fn) 
        .build()
        .map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?; 

        let gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(key.clone()),
            gossipsub_config,
        )?;

        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), key.public().to_peer_id())?;
        Ok(MyBehaviour { gossipsub, mdns })
    })?.with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(60)))
    .build();


    let topic = gossipsub::IdentTopic::new("group-chat");


    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    let mut stdin = io::BufReader::new(io::stdin()).lines();
    swarm.listen_on("/ip4/0.0.0.0/tcp/0".parse()?)?;



    loop {
        select! {
            Ok(Some(line)) = stdin.next_line() => {

                if let Err(e) = swarm
                    .behaviour_mut().gossipsub
                    .publish(topic.clone(), line.as_bytes()) {
                    println!("error: {:?}", e);
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                    for (peer_id, _multiaddr) in list {
                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                    for (peer_id, _multiaddr) in list {
                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                    }
                },
                SwarmEvent::Behaviour(MyBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                    propagation_source: _peer_id,
                    message_id: _id,
                    message,
                })) => println!("-{}", String::from_utf8_lossy(&message.data)),
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("Connected {address}");
                }
                _ => {}
            }
        }
    }
}
